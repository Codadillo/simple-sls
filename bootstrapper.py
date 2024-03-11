from pwn import *

context.arch = "amd64"

code = """
lea rax, [rip];
jmp rax;
"""

a = asm(code)

print(list(a))
