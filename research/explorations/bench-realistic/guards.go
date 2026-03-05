package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

//go:noinline
func classify(x int64) int64 {
	if x >= 900 { return 30 }
	if x >= 700 { return 25 }
	if x >= 500 { return 20 }
	if x >= 300 { return 15 }
	if x >= 100 { return 10 }
	return 5
}

func bench(n int64) int64 {
	var s int64
	for i := int64(0); i < n; i++ {
		s += classify(i)
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
