<script setup>
import { ref, onMounted } from 'vue'
import { NCard, NStatistic, NGrid, NGi, NTag, NSpace, NButton } from 'naive-ui'
import { getStatus } from '../api.js'

const emit = defineEmits(['note'])
const stats = ref(null)

async function load() {
  try {
    stats.value = await getStatus()
  } catch (e) {
    emit('note', { type: 'error', text: 'load status: ' + (e.response?.data || e.message) })
  }
}

const statusType = (s) =>
  s === 'active' ? 'success' : s === 'exhausted' || s === 'auth-failed' ? 'error' : 'warning'

onMounted(load)
</script>

<template>
  <n-space vertical v-if="stats">
    <n-space>
      <n-button @click="load">Reload</n-button>
    </n-space>
    <n-grid x-gap="16" y-gap="16" cols="3" responsive="screen">
      <n-gi><n-card><n-statistic label="Total keys" :value="stats.total_keys" /></n-card></n-gi>
      <n-gi><n-card><n-statistic label="Total users" :value="stats.total_users" /></n-card></n-gi>
      <n-gi>
        <n-card title="Keys by status">
          <n-space>
            <n-tag
              v-for="(n, s) in stats.keys_by_status"
              :key="s"
              :type="statusType(s)"
              size="medium"
            >{{ s }}: {{ n }}</n-tag>
          </n-space>
        </n-card>
      </n-gi>
    </n-grid>
  </n-space>
</template>
