<template>
  <button @click="onClick">{{ label }}</button>
</template>

<script setup>
const props = defineProps<{ label: string }>()
const emit = defineEmits<{
  (e: 'click', id: number): void
}>()
const inst = getCurrentInstance()
defineExpose({ focus: () => inst?.proxy?.$el?.focus() })

function onClick() {
  emit('click', 1)
}
</script>
