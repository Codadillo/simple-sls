from pwn import *

context.arch = "amd64"

test = "lea rax, [rip]; jmp rax"

a = asm(test)

print(list(a))
