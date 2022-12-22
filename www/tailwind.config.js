/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./*.{html,js}", "./.bin/*.js"],
  theme: {
    extend: {},
  },
  plugins: [require("daisyui")],
}
