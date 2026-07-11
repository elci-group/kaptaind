<?php
namespace App;

interface Logger {
    public function log(string $message): void;
}

trait Timestampable {
    public $createdAt;
    public static function now() {}
}
