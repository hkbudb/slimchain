SRC = $(wildcard contracts/*.sol)

TARGET = \
	build/contracts/IO.json \
	build/contracts/KVstore.json \
	build/contracts/Nothing.json \
	build/contracts/SimpleStorage.json \
	build/contracts/SmallBank.json \
	build/contracts/Sorter.json


build: $(TARGET)
.PHONY: build

$(TARGET) &: $(SRC)
	./compile.sh

clean:
	-rm -rf build
.PHONY: clean
