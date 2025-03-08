const colors = require('tailwindcss/colors');
const layerstack = require('@layerstack/tailwind/plugin');

module.exports = {
	content: [
		'./src/**/*.{html,svelte}',
		'./node_modules/svelte-ux/**/*.{svelte,js}'
	],

	// See customization docs: https://svelte-ux.techniq.dev/customization
	ux: {
		themes: {
			// light mode
			light: {
				primary: 'oklch(49.12% 0.3096 275.75)',
				secondary: 'oklch(69.71% 0.329 342.55)',
				'secondary-content': 'oklch(98.71% 0.0106 342.55)',
				accent: 'oklch(76.76% 0.184 183.61)',
				neutral: '#2B3440',
				'neutral-content': '#D7DDE4',
				'surface-100': 'oklch(100% 0 0)',
				'surface-200': '#F2F2F2',
				'surface-300': '#E5E6E6',
				'surface-content': '#1f2937',
			},

			// dark mode
			dark: {
				primary: 'oklch(65.69% 0.196 275.75)',
				secondary: 'oklch(74.8% 0.26 342.55)',
				accent: 'oklch(74.51% 0.167 183.61)',
				neutral: '#2a323c',
				'neutral-content': '#A6ADBB',
				'surface-100': '#1d232a',
				'surface-200': '#191e24',
				'surface-300': '#15191e',
				'surface-content': '#A6ADBB',
			},
		},
	},

	plugins: [
		layerstack,  // uses hsl() color space by default. To use oklch(), use: layerstack({ colorSpace: 'oklch' }),
	]
}; 
