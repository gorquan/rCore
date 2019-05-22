CC=gcc
CFLAGS=-O2 -std=c99
OBJ:=obj
SRC:=src
MODULE_NAME := $(notdir $(patsubst %/,%,$(CURDIR)))

SOURCES := $(wildcard $(SRC)/*.c)
OBJECTS := $(patsubst $(SRC)/%.c,$(OBJ)/%.o, $(SOURCES))

all: $(MODULE_NAME).ko
$(MODULE_NAME).ko: $(OBJECTS) obj/lkm_info.o
	$(CC) -shared -o $@ -nostdlib $(OBJECTS) obj/lkm_info.o
$(OBJ)/%.o:$(SRC)/%.c
	$(CC) $(CFLAGS) -fpic -nostdlib -c $< -o $@
obj/lkm_info.o: lkm_info.txt
	objcopy --input binary --output elf64-x86-64 \
		--binary-architecture i386:x86-64\
		--rename-section .data=.rcore-lkm,CONTENTS,READONLY\
		lkm_info.txt obj/lkm_info.o
	strip obj/lkm_info.o

clean:
	rm -fr $(OBJECTS) obj/lkm_info.o $(MODULE_NAME).ko

.PHONY: clean
