<script setup>
import { h, ref, onMounted } from 'vue'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NSelect,
  NTag,
  NPopconfirm,
} from 'naive-ui'
import { getKeys, addKey, removeKey } from '../api.js'

const emit = defineEmits(['note'])
const keys = ref([])
const loading = ref(false)
const form = ref({ provider: 'firecrawl', key: '', account: '' })

const statusType = (s) =>
  s === 'active' ? 'success' : s === 'exhausted' || s === 'auth-failed' ? 'error' : 'warning'

const columns = [
  { title: 'id', key: 'id', width: 50 },
  { title: 'provider', key: 'provider', width: 100 },
  { title: 'account', key: 'account_team', width: 120, render: (r) => r.account_team || '-' },
  {
    title: 'status',
    key: 'status',
    width: 110,
    render: (row) => h(NTag, { type: statusType(row.status), size: 'small' }, { default: () => row.status }),
  },
  { title: 'credits', key: 'credits_remaining', width: 80, render: (r) => r.credits_remaining ?? '-' },
  { title: 'cooldown', key: 'cooldown_until', width: 130, render: (r) => r.cooldown_until ?? '-' },
  { title: 'last_error', key: 'last_error', width: 150, render: (r) => r.last_error || '-' },
  { title: 'key', key: 'key_masked', width: 160 },
  {
    title: '',
    key: 'actions',
    width: 90,
    render: (row) =>
      h(
        NPopconfirm,
        { onPositiveClick: () => del(row.id) },
        {
          trigger: () => h(NButton, { size: 'small', type: 'error' }, { default: () => 'remove' }),
          default: () => `remove key ${row.id}?`,
        }
      ),
  },
]

async function load() {
  loading.value = true
  try {
    keys.value = await getKeys()
  } catch (e) {
    emit('note', { type: 'error', text: 'load keys: ' + (e.response?.data || e.message) })
  } finally {
    loading.value = false
  }
}

async function add() {
  try {
    await addKey({
      provider: form.value.provider,
      key: form.value.key,
      account: form.value.account || undefined,
    })
    form.value.key = ''
    await load()
    emit('note', { type: 'success', text: 'key added' })
  } catch (e) {
    emit('note', { type: 'error', text: 'add: ' + (e.response?.data || e.message) })
  }
}

async function del(id) {
  try {
    await removeKey(id)
    await load()
    emit('note', { type: 'success', text: 'key removed' })
  } catch (e) {
    emit('note', { type: 'error', text: 'remove: ' + (e.response?.data || e.message) })
  }
}

onMounted(load)
</script>

<template>
  <n-space vertical>
    <n-space>
      <n-select
        v-model:value="form.provider"
        :options="[{ label: 'firecrawl', value: 'firecrawl' }, { label: 'tavily', value: 'tavily' }]"
        style="width: 140px"
      />
      <n-input v-model:value="form.key" placeholder="key (fc-... / tvly-...)" style="width: 280px" />
      <n-input v-model:value="form.account" placeholder="account (optional)" style="width: 180px" />
      <n-button type="primary" @click="add">Add</n-button>
      <n-button @click="load">Reload</n-button>
    </n-space>
    <n-data-table :columns="columns" :data="keys" :loading="loading" :bordered="false" size="small" />
  </n-space>
</template>
