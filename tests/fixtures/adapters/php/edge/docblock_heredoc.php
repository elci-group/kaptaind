<?php
class Demo {
    /**
     * Mentions public function fake() in docs only.
     */
    public function real() {}

    public function sample() {
        $sql = <<<SQL
public function insideHeredoc() {}
SQL;
    }
}
