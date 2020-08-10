pragma solidity >=0.4.0 <0.7.0;

contract IO {
    bytes constant ALPHABET = "abcdefghijklmnopqrstuvwxy#$%^&*()_+[]{}|;:,./<>?`~abcdefghijklmnopqrstuvwxy#$%^&*()_+[]{}|;:,./<>?`~abcdefghijklmnopqrstuvwxy#$%^&*()_+[]{}|;:,./<>?`~";

    function getKey(uint256 k) internal pure returns (bytes32) {
        return bytes32(k);
    }

    function getVal(uint256 k) internal pure returns (bytes memory ret) {
        ret = new bytes(100);
        for (uint256 i = 0; i < 100; i++) {
            ret[i] = ALPHABET[(k % 50) + i];
        }
    }

    mapping(bytes32 => bytes) store;

    function get(bytes32 key) public view returns (bytes memory) {
        return store[key];
    }

    function set(bytes32 key, bytes memory value) public {
        store[key] = value;
    }

    function write(uint256 start_key, uint256 size) public {
        for (uint256 i = 0; i < size; i++) {
            set(getKey(start_key + i), getVal(start_key + i));
        }
    }

    function scan(uint256 start_key, uint256 size) public view {
        bytes memory ret;
        for (uint256 i = 0; i < size; i++) {
            ret = get(getKey(start_key + i));
        }
    }

    function revert_scan(uint256 start_key, uint256 size) public view {
        bytes memory ret;
        for (uint256 i = 0; i < size; i++) {
            ret = get(getKey(start_key + size - i - 1));
        }
    }
}
