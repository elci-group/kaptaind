// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Token {
    string public name;
    uint256 private supply;

    event Transfer(address indexed from, address indexed to, uint256 value);
    error InsufficientBalance(address account, uint256 available);

    constructor(string memory tokenName) {
        name = tokenName;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }

    function mint(uint256 amount) internal {
        supply += amount;
    }
}
