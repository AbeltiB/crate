import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#1B1A22",
        ink2: "#26242F",
        vellum: "#EDE6D8",
        wax: "#D9A441",
        oxblood: "#7A2E22",
        signal: "#2FB6A5",
        slate: "#8A8E9C",
      },
      fontFamily: {
        display: ["'Instrument Serif'", "serif"],
        sans: ["'IBM Plex Sans'", "sans-serif"],
        mono: ["'IBM Plex Mono'", "monospace"],
      },
    },
  },
  plugins: [],
} satisfies Config;
