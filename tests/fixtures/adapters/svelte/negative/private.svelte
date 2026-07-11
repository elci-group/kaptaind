<script>
let count = 0;
const MAX = 10;
function inc() { count += 1; }
</script>

<button on:click={inc}>{count}</button>
