package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

type Vec3 struct{ x, y, z int64 }

func run(n int64) int64 {
	var s int64
	for i := int64(0); i < n; i++ {
		v := Vec3{i, i * 2, i * 3}
		d := v.x + v.y + v.z
		v2 := Vec3{v.x + 1, v.y, v.z}
		s += d + v2.x
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
		run(n)
	}
	iters := int64(10000)
	start := time.Now()
	var r int64
	for i := int64(0); i < iters; i++ {
		r = run(n)
	}
	elapsed := time.Since(start).Nanoseconds()
	fmt.Printf("result:     %d\n", r)
	fmt.Printf("iterations: %d\n", iters)
	fmt.Printf("total:      %.2fms\n", float64(elapsed)/1e6)
	fmt.Printf("per call:   %dns\n", elapsed/iters)
}
