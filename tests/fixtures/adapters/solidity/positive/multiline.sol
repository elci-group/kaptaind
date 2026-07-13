// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Vault {
    fallback() external payable {
    }

    receive() external payable {
    }

    function deposit(
        address token,
        uint256[] memory amounts
    ) external payable returns (uint256) {
        return amounts.length;
    }

    function withdraw(
        address to,
        uint256 amount
    )
        public
        returns (bool)
    {
        return true;
    }
}
