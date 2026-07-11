<?php
namespace App;

function helper() {}

interface Logger {
    public function log(string $message): void;
}

class User {
    public const ROLE_ADMIN = 'admin';
    public string $name;
    private string $password;
    public function save() {}
}
