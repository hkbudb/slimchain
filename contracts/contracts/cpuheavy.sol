pragma solidity >=0.4.0 <0.7.0;

contract Sorter {
    function sort(uint256 size) public {
        uint256[] memory data = new uint256[](size);
        for (uint256 x = 0; x < data.length; x++) {
            data[x] = size - x;
        }
        quickSort(data, 0, data.length - 1);
    }

    function quickSort(
        uint256[] memory arr,
        uint256 left,
        uint256 right
    ) internal {
        uint256 i = left;
        uint256 j = right;
        if (i == j) return;
        uint256 pivot = arr[left + (right - left) / 2];
        while (i <= j) {
            while (arr[i] < pivot) i++;
            while (pivot < arr[j]) j--;
            if (i <= j) {
                (arr[i], arr[j]) = (arr[j], arr[i]);
                i++;
                j--;
            }
        }
        if (left < j) quickSort(arr, left, j);
        if (i < right) quickSort(arr, i, right);
    }
}
