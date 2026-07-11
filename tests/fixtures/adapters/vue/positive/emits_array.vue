<template><button @click="emit('save')">Save</button></template>
<script setup>
const emit = defineEmits(['save', 'cancel'])
</script>
