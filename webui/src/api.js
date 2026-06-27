import axios from 'axios'

const api = axios.create({ baseURL: '/api/v1' })

export async function login(password) {
  const r = await api.post('/login', { password })
  const token = r.data.token
  api.defaults.headers.common['Authorization'] = 'Bearer ' + token
  localStorage.setItem('sp_admin', token)
  return token
}

export function restoreSession() {
  const token = localStorage.getItem('sp_admin')
  if (token) {
    api.defaults.headers.common['Authorization'] = 'Bearer ' + token
    return true
  }
  return false
}

export function logout() {
  localStorage.removeItem('sp_admin')
  delete api.defaults.headers.common['Authorization']
}

export const getKeys = () => api.get('/keys').then((r) => r.data)
export const addKey = (body) => api.post('/keys', body).then((r) => r.data)
export const removeKey = (id) => api.delete('/keys/' + id)
export const getUsers = () => api.get('/users').then((r) => r.data)
export const createUser = (body) => api.post('/users', body).then((r) => r.data)
export const revokeUser = (token) => api.delete('/users/' + token)
export const getStatus = () => api.get('/status').then((r) => r.data)
