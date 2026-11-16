use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use log::{info, error};

use crate::device::{get_target, DeviceManager};
use crate::storage::{Schedule, Storage};
use crate::validation;

pub struct SchedulerState {
    scheduler: JobScheduler,
    job_ids: HashMap<String, uuid::Uuid>,
    device_manager: DeviceManager,
    storage: Arc<Storage>,
}

pub type SchedulerManager = Arc<RwLock<SchedulerState>>;

impl SchedulerState {
    pub async fn new(device_manager: DeviceManager, storage: Arc<Storage>) -> Result<Self, String> {
        let scheduler = JobScheduler::new().await
            .map_err(|e| format!("Failed to create scheduler: {}", e))?;

        Ok(Self {
            scheduler,
            job_ids: HashMap::new(),
            device_manager,
            storage,
        })
    }

    /// Convert a Schedule's time + days into a cron expression
    fn schedule_to_cron(schedule: &Schedule) -> Result<String, String> {
        let (hour, minute) = validation::schedule_time(&schedule.time)?;
        validation::schedule_days(&schedule.days)?;

        let days_of_week = if schedule.days.is_empty() {
            "*".to_string()
        } else {
            // Cron uses 0=Sun, 1=Mon..6=Sat but our format is 0=Mon..6=Sun
            // Convert: our 0(Mon)=cron 1, our 6(Sun)=cron 0
            schedule.days.iter()
                .map(|d| match d {
                    6 => 0, // Sunday
                    d => d + 1, // Mon=1..Sat=6
                })
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };

        // sec min hour day_of_month month day_of_week
        Ok(format!("0 {} {} * * {}", minute, hour, days_of_week))
    }

    pub async fn add_schedule_job(&mut self, schedule: &Schedule) -> Result<(), String> {
        if !schedule.enabled {
            return Ok(());
        }

        // Remove existing job if any
        self.remove_schedule_job(&schedule.id).await.ok();

        let cron_expr = Self::schedule_to_cron(schedule)?;
        let schedule_clone = schedule.clone();
        let dm = self.device_manager.clone();
        let storage = self.storage.clone();

        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _lock| {
            let sched = schedule_clone.clone();
            let dm = dm.clone();
            let storage = storage.clone();
            Box::pin(async move {
                execute_schedule(&sched, &dm, &storage).await;
            })
        }).map_err(|e| format!("Failed to create job: {}", e))?;

        let job_id = job.guid();
        self.scheduler.add(job).await
            .map_err(|e| format!("Failed to add job: {}", e))?;
        self.job_ids.insert(schedule.id.clone(), job_id);

        info!("Added schedule job: {} ({})", schedule.name, cron_expr);
        Ok(())
    }

    pub async fn remove_schedule_job(&mut self, schedule_id: &str) -> Result<(), String> {
        if let Some(job_id) = self.job_ids.remove(schedule_id) {
            self.scheduler.remove(&job_id).await
                .map_err(|e| format!("Failed to remove job: {}", e))?;
        }
        Ok(())
    }

    pub async fn reload_all_schedules(&mut self) -> Result<(), String> {
        // Clear all existing jobs
        let ids: Vec<String> = self.job_ids.keys().cloned().collect();
        for id in ids {
            self.remove_schedule_job(&id).await.ok();
        }

        // Load and add all enabled schedules
        let schedules = self.storage.get_schedules();
        for schedule in schedules {
            if schedule.enabled {
                if let Err(e) = self.add_schedule_job(&schedule).await {
                    error!("Skipping schedule {}: {}", schedule.name, e);
                }
            }
        }
        Ok(())
    }

    pub async fn start(&self) -> Result<(), String> {
        self.scheduler.start().await
            .map_err(|e| format!("Failed to start scheduler: {}", e))
    }
}

/// Execute a scheduled action on a device
async fn execute_schedule(schedule: &Schedule, device_manager: &DeviceManager, storage: &Storage) {
    let device = match get_target(device_manager, schedule.device_ip.as_deref()).await {
        Ok(d) => d,
        Err(e) => {
            error!("Cannot execute schedule {}: {}", schedule.name, e);
            return;
        }
    };

    let result = match schedule.action.as_str() {
        "on" => match schedule.brightness.map(validation::brightness).transpose() {
            Err(e) => Err(e),
            Ok(brightness) => match device.turn_on().await {
                Err(e) => Err(e),
                Ok(()) => match brightness {
                    Some(b) => device.set_brightness(b).await,
                    None => Ok(()),
                },
            },
        },
        "off" => device.turn_off().await,
        "preset" => match schedule.preset_id.as_deref() {
            Some(preset_id) => match storage.get_preset(preset_id) {
                Some(preset) => device.apply_preset(&preset).await,
                None => Err(format!("Preset {} not found", preset_id)),
            },
            None => Err("No preset_id for preset action".to_string()),
        },
        _ => Err(format!("Unknown action: {}", schedule.action)),
    };

    match result {
        Ok(()) => info!("Executed schedule: {}", schedule.name),
        Err(e) => error!("Failed to execute schedule {}: {}", schedule.name, e),
    }
}

pub async fn create_scheduler_manager(
    device_manager: DeviceManager,
    storage: Arc<Storage>,
) -> Result<SchedulerManager, String> {
    let mut state = SchedulerState::new(device_manager, storage).await?;
    state.reload_all_schedules().await?;
    state.start().await?;
    Ok(Arc::new(RwLock::new(state)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule(time: &str, days: Vec<i32>) -> Schedule {
        Schedule {
            id: "s1".into(), name: "test".into(), time: time.into(), days,
            action: "on".into(), preset_id: None, brightness: None, enabled: true,
            created_at: String::new(), device_ip: None,
        }
    }

    #[test]
    fn cron_maps_monday_based_days() {
        let cron = SchedulerState::schedule_to_cron(&schedule("07:30", vec![0, 6])).unwrap();
        assert_eq!(cron, "0 30 7 * * 1,0");
        let every_day = SchedulerState::schedule_to_cron(&schedule("23:05", vec![])).unwrap();
        assert_eq!(every_day, "0 5 23 * * *");
    }

    #[test]
    fn cron_rejects_invalid_input() {
        assert!(SchedulerState::schedule_to_cron(&schedule("7", vec![])).is_err());
        assert!(SchedulerState::schedule_to_cron(&schedule("*/1:00", vec![])).is_err());
        assert!(SchedulerState::schedule_to_cron(&schedule("07:00", vec![9])).is_err());
    }
}
