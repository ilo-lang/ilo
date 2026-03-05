package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

func fib(n int64) int64 {
	if n <= 1 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

func main() {
	n := int64(25)
	if len(os.Args) > 1 {
		if v, err := strconv.ParseInt(os.Args[1], 10, 64); err == nil {
			n = v
		}
	}
	for i := 0; i < 100; i++ {
		fib(n)
	}
	iters := int64(1000)
	start := time.Now()
	var r int64
	for i := int64(0); i < iters; i++ {
		r = fib(n)
	}
	elapsed := time.Since(start).Nanoseconds()
	fmt.Printf("result:     %d\n", r)
	fmt.Printf("iterations: %d\n", iters)
	fmt.Printf("total:      %.2fms\n", float64(elapsed)/1e6)
	fmt.Printf("per call:   %dns\n", elapsed/iters)
}
