/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        "agora-blue": "#1a56db",
        "agora-indigo": "#4c51bf",
        "agora-teal": "#0694a2",
        "agora-green": "#057a55",
        "agora-yellow": "#c27803",
        "agora-red": "#c81e1e",
      },
    },
  },
  plugins: [],
}; 