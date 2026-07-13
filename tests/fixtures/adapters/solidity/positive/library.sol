// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

struct Point {
    uint256 x;
    uint256 y;
}

enum State {
    Open,
    Closed
}

library Geometry {
    modifier validPoint(Point memory p) {
        _;
    }

    error Origin();

    function distance(Point memory a, Point memory b) public pure returns (uint256) {
        return 0;
    }
}

function origin() pure returns (Point memory) {
    return Point(0, 0);
}
