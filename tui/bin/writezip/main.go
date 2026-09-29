// Command writezip packs a directory of release files into a zip.
// tui/bin/package runs it (go run ./bin/writezip) for the Windows archive.
// It is a release helper, not a shipped tool.
package main

import (
	"archive/zip"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"time"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: writezip <archive.zip> <dir>")
		os.Exit(2)
	}
	if err := writeZip(os.Args[1], os.Args[2]); err != nil {
		fmt.Fprintf(os.Stderr, "writezip: %v\n", err)
		os.Exit(1)
	}
}

func writeZip(archive, dir string) error {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		names = append(names, entry.Name())
	}
	sort.Strings(names)

	out, err := os.Create(archive)
	if err != nil {
		return err
	}
	defer out.Close()

	zw := zip.NewWriter(out)
	for _, name := range names {
		if err := addFile(zw, filepath.Join(dir, name), name); err != nil {
			_ = zw.Close()
			return err
		}
	}
	return zw.Close()
}

func addFile(zw *zip.Writer, path, name string) error {
	in, err := os.Open(path)
	if err != nil {
		return err
	}
	defer in.Close()

	info, err := in.Stat()
	if err != nil {
		return err
	}
	header, err := zip.FileInfoHeader(info)
	if err != nil {
		return err
	}
	header.Name = name
	header.Method = zip.Deflate
	header.Modified = time.Date(1980, 1, 1, 0, 0, 0, 0, time.UTC)
	writer, err := zw.CreateHeader(header)
	if err != nil {
		return err
	}
	_, err = io.Copy(writer, in)
	return err
}
