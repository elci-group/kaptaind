// SPDX-License-Identifier: MIT
// contract FakeOne {
//     function hacked() external {}
// }

/*
interface IFake {
    function stolen() external;
}
*/

contract Real {
    /// NatSpec documents the function: function documented() external {}
    function genuine() external view returns (uint256) {
        return 1;
    }
}
