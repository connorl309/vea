; bubble_sort.s - the showpiece: sort a 5-byte array in place with nested loops
; and in-memory swaps, then read it back. Input [5,2,8,1,3] -> [1,2,3,5,8].

        mov   r1, 0x4000      ;= r1=16384    ; array base
        mov   r2, #5
        st.b  [r1], r2
        mov   r2, #2
        st.b  [r1 + #1], r2
        mov   r2, #8
        st.b  [r1 + #2], r2
        mov   r2, #1
        st.b  [r1 + #3], r2
        mov   r2, #3
        st.b  [r1 + #4], r2

        mov   r3, #5         ; n
        mov   r4, #0         ; i (outer pass)
outer:  mov   r5, #0        ; j (inner index)
        sub   r8, r3, r4    ; n - i
        sub   r8, r8, #1    ; last j to touch this pass

inner:  ld.b  r6, [r1 + r5]        ; a[j]
        add   r9, r5, #1
        ld.b  r7, [r1 + r9]        ; a[j+1]
        cmp   r6, r7
        ble   noswap              ; already ordered
        st.b  [r1 + r5], r7       ; swap a[j], a[j+1]
        st.b  [r1 + r9], r6
noswap: add   r5, r5, #1
        cmp   r5, r8
        blt   inner

        add   r4, r4, #1
        cmp   r4, r3
        blt   outer

        ld.b  r10, [r1]
        ld.b  r11, [r1 + #1]
        ld.b  r12, [r1 + #2]
        ld.b  r13, [r1 + #3]
        ld.b  r14, [r1 + #4]
        halt

;= final: r10=1 r11=2 r12=3 r13=5 r14=8
