// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// The smallest stateful contract: one storage slot, one event.
contract Counter {
    uint256 public count;

    event Incremented(uint256 by, uint256 count);

    function increment(uint256 by) external {
        count += by;
        emit Incremented(by, count);
    }
}
