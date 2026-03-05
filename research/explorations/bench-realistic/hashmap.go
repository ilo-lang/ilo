package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

type Entry struct{ k, v, sum int64 }

func bench(n int64) int64 {
	var s int64
	for i := int64(0); i < n; i++ {
		e := Entry{i, i * 7, 0}
		e2 := Entry{e.k, e.v, e.k + e.v}
		s += e2.sum
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
