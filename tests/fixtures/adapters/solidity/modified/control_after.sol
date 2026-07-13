// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Token {
    function transfer(address to, uint256 amount) external returns (bool) {
        require(to != address(0));
        require(amount > 0);
        return true;
    }
}
