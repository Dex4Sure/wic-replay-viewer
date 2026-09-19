import { createApp } from 'vue';

import App from './App.vue';
import './styles.css';
import { installErrorReporting } from './errorReporting';

const app = createApp(App);
installErrorReporting(app);
app.mount('#app');
