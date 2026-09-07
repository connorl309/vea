; strlen.s - write "Hi!\0" as bytes at 0x5000, then scan for the NUL with an
; index-register byte load. Known answer: length 3.

        mov   r1, 0x5000     ;= r1=20480
        mov   r2, #72        ; 'H'
        st.b  [r1], r2
        mov   r2, #105       ; 'i'
        st.b  [r1 + #1], r2
        mov   r2, #33        ; '!'
        st.b  [r1 + #2], r2
        mov   r2, #0
        st.b  [r1 + #3], r2  ; terminator

        mov   r3, #0         ; length
scan:   ld.b  r4, [r1 + r3]
        cmp   r4, #0
        beq   done
        add   r3, r3, #1
        b     scan
done:   halt

;= final: r3=3 r4=0 mem@0x5000 = [72, 105, 33, 0]

