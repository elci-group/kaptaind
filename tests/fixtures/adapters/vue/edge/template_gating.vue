<template>
  <!-- defineEmits(['click']) lives in template and is ignored -->
  <button>{{ label }}</button>
</template>

<script setup>
const props = defineProps<{ label: string }>()
</script>
