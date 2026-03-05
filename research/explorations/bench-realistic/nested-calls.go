package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

//go:noinline
func addn(a, b int64) int64 { return a + b }

//go:noinline
func muln(a, b int64) int64 { return a * b }

//go:noinline
func compute(x, y int64) int64 {
	a := muln(x, y)
	b := addn(a, x)
	return addn(b, y)
}

func bench(n int64) int64 {
	var s, i int64
	for i < n {
		j := i + 1
		s += compute(i, j)
		i++
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
