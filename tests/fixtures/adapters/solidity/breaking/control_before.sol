// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Token {
    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }

    function normalize(uint256 amount) internal pure returns (uint256) {
        return amount;
    }
}
