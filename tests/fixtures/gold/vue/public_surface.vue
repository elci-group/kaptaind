<template>
  <div>hi</div>
</template>

<script setup>
const props = defineProps({
  title: String,
});
const emit = defineEmits(["save"]);
defineExpose({ focus });
</script>
