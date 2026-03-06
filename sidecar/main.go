//go:build ignore

package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Println("gRPC UI FFI Library")
	fmt.Println("")
	fmt.Println("This is no longer a standalone executable.")
	fmt.Println("The FFI library is built as a shared library (.dylib/.so/.dll)")
	fmt.Println("and loaded by the Rust Tauri application.")
	fmt.Println("")
	fmt.Println("Build the FFI library with:")
	fmt.Println("  ./scripts/build-ffi.sh")
	os.Exit(1)
}
