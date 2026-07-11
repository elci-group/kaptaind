<template>
  <span>{{ _label }}</span>
</template>

<script>
const _label = 'internal'
let _helper = () => _label.toUpperCase()

function _format() {
  return _helper()
}
</script>
