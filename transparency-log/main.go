// A minimal Tessera-backed transparency log.
package main

import (
	"context"
	"crypto/rand"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/transparency-dev/tessera"
	"github.com/transparency-dev/tessera/storage/posix"
	"golang.org/x/mod/sumdb/note"
)

func main() {
	addr := flag.String("listen", "127.0.0.1:8082", "HTTP listen address")
	dir := flag.String("storage", "transparency-data", "Tessera storage directory")
	key := flag.String("private-key", "", "file containing a Go note signing key")
	generate := flag.Bool("generate-key", false, "generate a signing key pair and exit")
	flag.Parse()
	if *generate {
		secret, public, err := note.GenerateKey(rand.Reader, "translog-nix")
		if err != nil {
			log.Fatal(err)
		}
		fmt.Printf("Private: %s\nPublic: %s\n", secret, public)
		return
	}
	b, err := os.ReadFile(*key)
	if err != nil {
		log.Fatal(err)
	}
	signer, err := note.NewSigner(strings.TrimSpace(string(b)))
	if err != nil {
		log.Fatal(err)
	}
	ctx := context.Background()
	driver, err := posix.New(ctx, posix.Config{Path: *dir})
	if err != nil {
		log.Fatal(err)
	}
	appender, shutdown, _, err := tessera.NewAppender(ctx, driver, tessera.NewAppendOptions().WithCheckpointSigner(signer).WithCheckpointInterval(time.Second).WithBatching(256, time.Second))
	if err != nil {
		log.Fatal(err)
	}
	defer shutdown(ctx)
	mux := http.NewServeMux()
	mux.HandleFunc("POST /add", func(w http.ResponseWriter, r *http.Request) {
		b, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 65535))
		if err != nil {
			http.Error(w, "entry exceeds 65535 bytes or could not be read", http.StatusBadRequest)
			return
		}
		idx, err := appender.Add(r.Context(), tessera.NewEntry(b))()
		if err != nil {
			http.Error(w, "append failed", http.StatusInternalServerError)
			return
		}
		fmt.Fprintln(w, idx.Index)
	})
	fs := http.FileServer(http.Dir(*dir))
	mux.Handle("GET /checkpoint", fs)
	mux.Handle("GET /tile/", fs)
	mux.Handle("GET /entries/", fs)
	log.Fatal((&http.Server{Addr: *addr, Handler: mux, ReadHeaderTimeout: 5 * time.Second}).ListenAndServe())
}
