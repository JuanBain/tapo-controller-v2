use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use log::{info, error};

use crate::device::DeviceManager;
use crate::storage::{Schedule, Storage};

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
    fn schedule_to_cron(schedule: &Schedule) -> String {
        let parts: Vec<&str> = schedule.time.split(':').collect();
        let hour = parts.get(0).unwrap_or(&"0");
        let minute = parts.get(1).unwrap_or(&"0");

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
        format!("0 {} {} * * {}", minute, hour, days_of_week)
    }

    pub async fn add_schedule_job(&mut self, schedule: &Schedule) -> Result<(), String> {
        if !schedule.enabled {
            return Ok(());
        }

        // Remove existing job if any
        self.remove_schedule_job(&schedule.id).await.ok();

        let cron_expr = Self::schedule_to_cron(schedule);
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
                self.add_schedule_job(&schedule).await?;
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
    let devices = device_manager.read().await;
    let device = match devices.values().next() {
        Some(d) => d,
        None => {
            error!("Cannot execute schedule {}: No device connected", schedule.name);
            return;
        }
    };

    let result = match schedule.action.as_str() {
        "on" => {
            if let Err(e) = device.turn_on().await {
                error!("Schedule {} turn_on failed: {}", schedule.name, e);
                return;
            }
            if let Some(brightness) = schedule.brightness {
                device.set_brightness(brightness as u8).await
            } else {
                Ok(())
            }
        }
        "off" => device.turn_off().await,
        "preset" => {
            if let Some(ref preset_id) = schedule.preset_id {
                if let Some(preset) = storage.get_preset(preset_id) {
                    if let Err(e) = device.turn_on().await {
                        error!("Schedule {} turn_on failed: {}", schedule.name, e);
                        return;
                    }
                    if let Err(e) = device.set_brightness(preset.brightness as u8).await {
                        error!("Schedule {} set_brightness failed: {}", schedule.name, e);
                        return;
                    }
                    if let (Some(hue), Some(sat)) = (preset.hue, preset.saturation) {
                        if let Err(e) = device.set_color(hue as u16, sat as u8).await {
                            error!("Schedule {} set_color failed: {}", schedule.name, e);
                        }
                    } else if let Some(temp) = preset.color_temperature {
                        if let Err(e) = device.set_color_temperature(temp as u16).await {
                            error!("Schedule {} set_temp failed: {}", schedule.name, e);
                        }
                    }
                    if let Some(ref effect) = preset.effect {
                        if let Err(e) = device.set_effect(effect).await {
                            error!("Schedule {} set_effect failed: {}", schedule.name, e);
                        }
                    }
                    Ok(())
                } else {
                    Err(format!("Preset {} not found", preset_id))
                }
            } else {
                Err("No preset_id for preset action".to_string())
            }
        }
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
