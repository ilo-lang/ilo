package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

//go:noinline
func cata(x int64) int64 {
	if x >= 800 { if x >= 900 { return 9 }; return 8 }
	if x >= 600 { if x >= 700 { return 7 }; return 6 }
	if x >= 400 { if x >= 500 { return 5 }; return 4 }
	if x >= 200 { if x >= 300 { return 3 }; return 2 }
	return 1
}

//go:noinline
func catb(x int64) int64 {
	if x >= 500 { return x * 3 }
	if x >= 200 { return x * 2 }
	return x
}

//go:noinline
func combine(a, b int64) int64 {
	if a >= 7 { return b + a*10 }
	if a >= 4 { return b + a*5 }
	return b + a
}

func bench(n int64) int64 {
	var s int64
	for i := int64(0); i < n; i++ {
		a := cata(i)
		b := catb(i)
		c := combine(a, b)
		s += c
	}
	return s
}

func main() {
	n := int64(1000)
	if len(os.Args) > 1 {
		if v, err := strconv.ParseInt(os.Args[1], 10, 64); err == nil {
			n = v
		}
	}
	for i := 0; i < 1000; i++ {
		bench(n)
	}
	iters := int64(10000)
	start := time.Now()
	var r int64
	for i := int64(0); i < iters; i++ {
		r = bench(n)
	}
	elapsed := time.Since(start).Nanoseconds()
	fmt.Printf("result:     %d\n", r)
	fmt.Printf("iterations: %d\n", iters)
	fmt.Printf("total:      %.2fms\n", float64(elapsed)/1e6)
	fmt.Printf("per call:   %dns\n", elapsed/iters)
}
