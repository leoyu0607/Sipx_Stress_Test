<script setup lang="ts">
import { ref, provide, watch } from 'vue'
import TitleBar    from './components/TitleBar.vue'
import Sidebar     from './components/Sidebar.vue'
import MetricStrip from './components/MetricStrip.vue'
import ChartPanel  from './components/ChartPanel.vue'
import RightPanel  from './components/RightPanel.vue'
import LogPanel    from './components/LogPanel.vue'

// ── Theme ─────────────────────────────────────────────────────────
const isDark = ref(true)

function toggleTheme() {
  isDark.value = !isDark.value
  document.documentElement.setAttribute('data-theme', isDark.value ? 'dark' : 'light')
}

// Set initial attribute
document.documentElement.setAttribute('data-theme', 'dark')

// Provide to all children
provide('isDark', isDark)
provide('toggleTheme', toggleTheme)
</script>

<template>
  <div class="app">
    <TitleBar />
    <div class="body">
      <Sidebar />
      <div class="content">
        <MetricStrip />
        <div class="middle">
          <ChartPanel />
          <RightPanel />
        </div>
        <LogPanel />
      </div>
    </div>
  </div>
</template>

<style>
/* ══════════════════════════════════════════════
   DARK THEME (default)
══════════════════════════════════════════════ */
:root,
[data-theme="dark"] {
  --bg0:     #0b0d12;
  --bg1:     #111318;
  --bg2:     #171a22;
  --bg3:     #1e2230;
  --bg4:     #252a38;

  --border:  rgba(255,255,255,0.06);
  --border2: rgba(255,255,255,0.11);
  --border3: rgba(255,255,255,0.20);

  --accent:  #00e5c0;
  --blue:    #3d9fff;
  --warn:    #f5a623;
  --danger:  #ff4d4d;

  --text0: #e8eaf0;
  --text1: #7c8296;
  --text2: #464c5e;
  --text3: #2a2e3d;

  --input-bg:     #0b0d12;
  --input-border: rgba(255,255,255,0.11);

  --scrollbar-thumb: rgba(255,255,255,0.1);

  color-scheme: dark;
}

/* ══════════════════════════════════════════════
   LIGHT THEME
══════════════════════════════════════════════ */
[data-theme="light"] {
  --bg0:     #f0f2f5;
  --bg1:     #ffffff;
  --bg2:     #f5f6f8;
  --bg3:     #eaecf0;
  --bg4:     #dde0e6;

  --border:  rgba(0,0,0,0.07);
  --border2: rgba(0,0,0,0.12);
  --border3: rgba(0,0,0,0.20);

  --accent:  #00b89a;
  --blue:    #1a7fd4;
  --warn:    #d4860a;
  --danger:  #d93636;

  --text0: #1a1c22;
  --text1: #4a5068;
  --text2: #8892a4;
  --text3: #c0c6d4;

  --input-bg:     #ffffff;
  --input-border: rgba(0,0,0,0.14);

  --scrollbar-thumb: rgba(0,0,0,0.12);

  color-scheme: light;
}

/* ══════════════════════════════════════════════
   RESET & BASE
══════════════════════════════════════════════ */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

html, body, #app {
  height: 100%;
  overflow: hidden;
  background: var(--bg0);
  color: var(--text0);
  font-family: var(--sans);
  font-size: 13px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  transition: background 0.25s, color 0.25s;
}

/* ── Scrollbar ─────────────────────────────── */
::-webkit-scrollbar { width: 4px; height: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--scrollbar-thumb); border-radius: 2px; }

/* ── Shared Buttons ────────────────────────── */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 11px;
  border-radius: var(--radius);
  font-family: var(--mono);
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--border2);
  background: transparent;
  color: var(--text1);
  cursor: pointer;
  transition: background 0.14s, border-color 0.14s, color 0.14s, transform 0.1s;
  white-space: nowrap;
  outline: none;
}
.btn:hover { background: var(--bg3); color: var(--text0); border-color: var(--border3); }
.btn:active { transform: scale(0.97); }
.btn:disabled { opacity: 0.35; cursor: not-allowed; }

.btn-sm { padding: 3px 8px; font-size: 10px; }

.btn-accent {
  color: var(--accent);
  border-color: rgba(0,184,154,0.3);
  background: rgba(0,184,154,0.06);
}
.btn-accent:hover { background: rgba(0,184,154,0.14); border-color: rgba(0,184,154,0.5); color: var(--accent); }

.btn-danger {
  color: var(--danger);
  border-color: rgba(217,54,54,0.3);
  background: rgba(217,54,54,0.07);
}
.btn-danger:hover { background: rgba(217,54,54,0.16); border-color: rgba(217,54,54,0.5); }

/* ── Fonts (referenced by components) ─────── */
:root {
  --mono: 'JetBrains Mono', 'Cascadia Code', monospace;
  --sans: 'IBM Plex Sans', system-ui, sans-serif;
  --radius-sm: 4px;
  --radius:    6px;
  --radius-lg: 10px;
}

/* ── App Shell ─────────────────────────────── */
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
}
.body {
  display: flex;
  flex: 1;
  overflow: hidden;
}
.content {
  display: flex;
  flex-direction: column;
  flex: 1;
  overflow: hidden;
}
.middle {
  display: flex;
  flex: 1;
  overflow: hidden;
}
</style>
