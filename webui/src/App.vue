<script setup>
import { ref, onMounted } from 'vue'
import {
  NConfigProvider,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NButton,
  NTabs,
  NTabPane,
  NSpace,
  NAlert,
} from 'naive-ui'
import { login, restoreSession, logout, getStatus } from './api.js'
import KeysPane from './components/KeysPane.vue'
import UsersPane from './components/UsersPane.vue'
import StatusPane from './components/StatusPane.vue'

const logged = ref(false)
const password = ref('')
const loading = ref(false)
const note = ref(null)

function setNote(n) {
  note.value = n
  if (n) setTimeout(() => { if (note.value === n) note.value = null }, 4000)
}

async function doLogin() {
  loading.value = true
  note.value = null
  try {
    await login(password.value)
    logged.value = true
    password.value = ''
  } catch (e) {
    setNote({ type: 'error', text: 'login failed: ' + (e.response?.data || e.message) })
  } finally {
    loading.value = false
  }
}

function doLogout() {
  logout()
  logged.value = false
}

onMounted(async () => {
  if (restoreSession()) {
    try {
      await getStatus()
      logged.value = true
    } catch {
      /* invalid/expired session -> show login */
    }
  }
})
</script>

<template>
  <n-config-provider>
    <div style="max-width: 1000px; margin: 24px auto; padding: 0 16px;">
      <h2 style="margin: 4px 0 16px">search-proxy</h2>
      <n-alert
        v-if="note"
        :type="note.type"
        closable
        style="margin-bottom: 12px"
        @close="note = null"
      >{{ note.text }}</n-alert>

      <n-card v-if="!logged" title="Admin login" style="max-width: 380px">
        <n-form @submit.prevent="doLogin">
          <n-form-item label="Password">
            <n-input v-model:value="password" type="password" @keyup.enter="doLogin" />
          </n-form-item>
          <n-button type="primary" :loading="loading" @click="doLogin">Login</n-button>
        </n-form>
      </n-card>

      <div v-else>
        <n-space justify="end" style="margin-bottom: 12px">
          <n-button @click="doLogout">Logout</n-button>
        </n-space>
        <n-tabs type="line">
          <n-tab-pane name="keys" tab="Keys"><KeysPane @note="setNote" /></n-tab-pane>
          <n-tab-pane name="users" tab="Users"><UsersPane @note="setNote" /></n-tab-pane>
          <n-tab-pane name="status" tab="Status"><StatusPane @note="setNote" /></n-tab-pane>
        </n-tabs>
      </div>
    </div>
  </n-config-provider>
</template>
