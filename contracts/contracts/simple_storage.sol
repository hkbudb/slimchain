pragma solidity >=0.4.0 <0.7.0;

contract SimpleStorage {
    mapping(uint256 => uint256) public data;

    constructor() public {
        data[1] = 42;
    }

    function set(uint256 key, uint256 value) public {
        data[key] = value;
    }

    function get(uint256 key) public view returns (uint256) {
        return data[key];
    }
}
