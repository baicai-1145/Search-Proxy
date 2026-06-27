<script setup>
import { h, ref, onMounted } from 'vue'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NPopconfirm,
} from 'naive-ui'
import { getUsers, createUser, revokeUser } from '../api.js'

const emit = defineEmits(['note'])
const users = ref([])
const loading = ref(false)
const name = ref('')

const columns = [
  { title: 'id', key: 'id', width: 50 },
  { title: 'token', key: 'token' },
  { title: 'name', key: 'name', render: (r) => r.name || '-' },
  { title: 'created_at', key: 'created_at', width: 140 },
  {
    title: '',
    key: 'actions',
    width: 90,
    render: (row) =>
      h(
        NPopconfirm,
        { onPositiveClick: () => revoke(row.token) },
        {
          trigger: () => h(NButton, { size: 'small', type: 'error' }, { default: () => 'revoke' }),
          default: () => 'revoke this user token?',
        }
      ),
  },
]

async function load() {
  loading.value = true
  try {
    users.value = await getUsers()
  } catch (e) {
    emit('note', { type: 'error', text: 'load users: ' + (e.response?.data || e.message) })
  } finally {
    loading.value = false
  }
}

async function create() {
  try {
    await createUser({ name: name.value || undefined })
    name.value = ''
    await load()
    emit('note', { type: 'success', text: 'user created' })
  } catch (e) {
    emit('note', { type: 'error', text: 'create: ' + (e.response?.data || e.message) })
  }
}

async function revoke(token) {
  try {
    await revokeUser(token)
    await load()
    emit('note', { type: 'success', text: 'user revoked' })
  } catch (e) {
    emit('note', { type: 'error', text: 'revoke: ' + (e.response?.data || e.message) })
  }
}

onMounted(load)
</script>

<template>
  <n-space vertical>
    <n-space>
      <n-input v-model:value="name" placeholder="name (optional)" style="width: 220px" />
      <n-button type="primary" @click="create">Create user</n-button>
      <n-button @click="load">Reload</n-button>
    </n-space>
    <n-data-table :columns="columns" :data="users" :loading="loading" :bordered="false" size="small" />
  </n-space>
</template>
