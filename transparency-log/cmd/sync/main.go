// Sync a complete Tessera log into a verified local snapshot.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	logformat "github.com/transparency-dev/formats/log"
	"github.com/transparency-dev/merkle/compact"
	"github.com/transparency-dev/merkle/rfc6962"
	"github.com/transparency-dev/tessera/api/layout"
	"github.com/transparency-dev/tessera/client"
	"golang.org/x/mod/sumdb/note"
)

type snapshot struct {
	Checkpoint []byte   `json:"checkpoint"`
	Entries    [][]byte `json:"entries"`
}

func run(args []string) error {
	flags := flag.NewFlagSet("translog-sync", flag.ContinueOnError)
	address := flags.String("url", "http://127.0.0.1:8082/", "log base URL")
	keyFile := flags.String("public-key", "", "checkpoint public key file")
	file := flags.String("output", "translog-snapshot.json", "verified snapshot file")
	offline := flags.Bool("offline", false, "verify the saved snapshot without networking")
	if err := flags.Parse(args); err != nil {
		return err
	}
	key, err := os.ReadFile(*keyFile)
	if err != nil {
		return err
	}
	verifier, err := note.NewVerifier(strings.TrimSpace(string(key)))
	if err != nil {
		return err
	}
	origin := verifier.Name()
	if *offline {
		raw, err := os.ReadFile(*file)
		if err != nil {
			return err
		}
		var saved snapshot
		if err = json.Unmarshal(raw, &saved); err != nil {
			return err
		}
		cp, _, _, err := logformat.ParseCheckpoint(saved.Checkpoint, origin, verifier)
		if err != nil {
			return err
		}
		if uint64(len(saved.Entries)) != cp.Size {
			return fmt.Errorf("snapshot size mismatch")
		}
		hasher := rfc6962.DefaultHasher
		factory := compact.RangeFactory{Hash: hasher.HashChildren}
		tree := factory.NewEmptyRange(0)
		for _, entry := range saved.Entries {
			if err := tree.Append(hasher.HashLeaf(entry), nil); err != nil {
				return err
			}
		}
		root, err := tree.GetRootHash(nil)
		if err != nil {
			return err
		}
		if cp.Size == 0 {
			root = hasher.EmptyRoot()
		}
		if !bytes.Equal(root, cp.Hash) {
			return fmt.Errorf("snapshot root mismatch")
		}
		return nil
	}

	u, err := url.Parse(*address)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || !strings.HasSuffix(u.Path, "/") || u.RawQuery != "" || u.Fragment != "" {
		return fmt.Errorf("invalid log URL")
	}
	fetch, err := client.NewHTTPFetcher(u, &http.Client{Timeout: 30 * time.Second})
	if err != nil {
		return err
	}
	ctx := context.Background()
	checkpoint, raw, _, err := client.FetchCheckpoint(ctx, fetch.ReadCheckpoint, verifier, origin)
	if err != nil {
		return err
	}
	var prev snapshot
	prevRaw, err := os.ReadFile(*file)
	if err == nil {
		if err = json.Unmarshal(prevRaw, &prev); err != nil {
			return err
		}
	} else if !os.IsNotExist(err) {
		return err
	}
	var oldSize uint64
	var oldRoot []byte
	if len(prev.Checkpoint) > 0 {
		old, _, _, err := logformat.ParseCheckpoint(prev.Checkpoint, origin, verifier)
		if err != nil {
			return fmt.Errorf("old checkpoint: %w", err)
		}
		oldSize, oldRoot = old.Size, old.Hash
		if oldSize > checkpoint.Size {
			return fmt.Errorf("checkpoint rollback")
		}
	}
	hash := rfc6962.DefaultHasher
	factory := compact.RangeFactory{Hash: hash.HashChildren}
	tree := factory.NewEmptyRange(0)
	records := make([][]byte, 0)
	if oldSize == 0 && len(oldRoot) > 0 && !bytes.Equal(oldRoot, hash.EmptyRoot()) {
		return fmt.Errorf("old empty checkpoint mismatch")
	}
	for ri := range layout.Range(0, checkpoint.Size, checkpoint.Size) {
		bundle, err := client.GetEntryBundle(ctx, fetch.ReadEntryBundle, ri.Index, checkpoint.Size)
		if err != nil {
			return err
		}
		if uint(len(bundle.Entries)) < ri.First+ri.N {
			return fmt.Errorf("short entry bundle")
		}
		for _, entry := range bundle.Entries[ri.First : ri.First+ri.N] {
			if err := tree.Append(hash.HashLeaf(entry), nil); err != nil {
				return err
			}
			records = append(records, entry)
			if tree.End() == oldSize {
				root, err := tree.GetRootHash(nil)
				if err != nil || !bytes.Equal(root, oldRoot) {
					return fmt.Errorf("checkpoint consistency failure")
				}
			}
		}
	}
	root, err := tree.GetRootHash(nil)
	if err != nil {
		return err
	}
	if checkpoint.Size == 0 {
		root = hash.EmptyRoot()
	}
	if !bytes.Equal(root, checkpoint.Hash) {
		return fmt.Errorf("entries do not match checkpoint")
	}
	output, err := json.Marshal(snapshot{Checkpoint: raw, Entries: records})
	if err != nil {
		return err
	}
	dir := filepath.Dir(*file)
	if err = os.MkdirAll(dir, 0700); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(dir, ".snapshot-*")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err = tmp.Write(output); err != nil {
		tmp.Close()
		return err
	}
	if err = tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err = tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), *file)
}
func main() {
	if err := run(os.Args[1:]); err != nil {
		log.Fatal(err)
	}
}
