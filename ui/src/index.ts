import { hydrate } from 'svelte';
import Index from './routes/App.svelte';
import './app.css';

hydrate(Index, {
    target: document.getElementById('root')!,
    props: {},
});
