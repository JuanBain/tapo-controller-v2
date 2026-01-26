/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        dark: {
          50: '#f0f0f5',
          100: '#e0e0eb',
          200: '#c2c2d6',
          300: '#a3a3c2',
          400: '#8585ad',
          500: '#666699',
          600: '#52527a',
          700: '#3d3d5c',
          800: '#29293d',
          900: '#1a1a2e',
          950: '#0d0d17',
        },
      },
    },
  },
  plugins: [],
};
