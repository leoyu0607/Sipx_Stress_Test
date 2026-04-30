import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'

// Google Fonts (JetBrains Mono + IBM Plex Sans)
const link = document.createElement('link')
link.rel = 'stylesheet'
link.href = 'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@300;400;500;700&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap'
document.head.appendChild(link)

const app = createApp(App)
app.use(createPinia())
app.mount('#app')
