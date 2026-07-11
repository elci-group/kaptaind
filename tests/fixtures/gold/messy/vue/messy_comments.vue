<template>
  <div>{{ title }}</div>
</template>

<script setup>
// defineProps was here: defineProps({ legacy: String })
const props = defineProps({
  title: String,
});
</script>
