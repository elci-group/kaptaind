<?php
trait Cacheable {
    public function key(): string {
        return 'cache';
    }
}
