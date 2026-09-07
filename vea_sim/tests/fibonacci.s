; fibonacci.s - iterative Fibonacci that also writes the sequence to memory as
; 8-byte words at 0x3000 (fib(k) at 0x3000 + 8*k), then reads two entries back.
; Known answers: fib(2) = 1, fib(10) = 55.

        mov   r1, 0x3000      ;= r1=12288    ; moving write pointer
        mov   r2, #0          ; a = fib(0)
        mov   r3, #1          ; b = fib(1)
        st    [r1], r2        ; mem[0x3000] = 0
        add   r1, r1, #8
        st    [r1], r3        ; mem[0x3008] = 1
        add   r1, r1, #8
        mov   r4, #2          ; i
        mov   r5, #10         ; last index to compute

fib:    add   r6, r2, r3     ; c = a + b
        st    [r1], r6
        add   r1, r1, #8
        mov   r2, r3         ; a = b
        mov   r3, r6         ; b = c
        add   r4, r4, #1
        cmp   r4, r5
        ble   fib

        mov   r10, 0x3000
        ld    r11, [r10 + #16]   ; fib(2)
        ld    r12, [r10 + #80]   ; fib(10)
        halt

;= final: r3=55 r6=55 r4=11 r11=1 r12=55
; fib(0)=0 and fib(1)=1 as the first two big-endian 8-byte words
;= final: mem@0x3000 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
