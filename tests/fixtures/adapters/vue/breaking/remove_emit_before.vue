<template><button @click="emit('click')">x</button></template>
<script setup>
const emit = defineEmits(['click'])
</script>
