package main

import (
	"crypto/rand"
	"encoding/binary"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	logformat "github.com/transparency-dev/formats/log"
	"github.com/transparency-dev/merkle/compact"
	"github.com/transparency-dev/merkle/rfc6962"
	"golang.org/x/mod/sumdb/note"
)

func TestSyncRejectsTamperingAndRollback(t *testing.T) {
	private, public, err := note.GenerateKey(rand.Reader, "test-log")
	if err != nil {
		t.Fatal(err)
	}
	signer, err := note.NewSigner(private)
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	key := filepath.Join(dir, "key")
	output := filepath.Join(dir, "snapshot")
	if err = os.WriteFile(key, []byte(public), 0600); err != nil {
		t.Fatal(err)
	}
	entries := [][]byte{[]byte("first")}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.URL.Path == "/checkpoint":
			hash := rfc6962.DefaultHasher
			factory := compact.RangeFactory{Hash: hash.HashChildren}
			tree := factory.NewEmptyRange(0)
			for _, e := range entries {
				if err := tree.Append(hash.HashLeaf(e), nil); err != nil {
					t.Error(err)
				}
			}
			root, _ := tree.GetRootHash(nil)
			cp := logformat.Checkpoint{Origin: "test-log", Size: uint64(len(entries)), Hash: root}
			signed, err := note.Sign(&note.Note{Text: string(cp.Marshal())}, signer)
			if err != nil {
				t.Error(err)
			}
			w.Write(signed)
		case strings.HasPrefix(r.URL.Path, "/tile/entries"):
			for _, e := range entries {
				var size [2]byte
				binary.BigEndian.PutUint16(size[:], uint16(len(e)))
				w.Write(size[:])
				w.Write(e)
			}
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	args := []string{"-url", server.URL + "/", "-public-key", key, "-output", output}
	if err := run(args); err != nil {
		t.Fatal(err)
	}
	entries = append(entries, []byte("second"))
	if err := run(args); err != nil {
		t.Fatal(err)
	}
	entries = [][]byte{[]byte("different"), []byte("second"), []byte("third")}
	if err := run(args); err == nil {
		t.Fatal("accepted inconsistent history")
	}
	entries = entries[:1]
	if err := run(args); err == nil {
		t.Fatal("accepted rollback")
	}
	var saved snapshot
	b, err := os.ReadFile(output)
	if err != nil {
		t.Fatal(err)
	}
	if err = json.Unmarshal(b, &saved); err != nil {
		t.Fatal(err)
	}
	saved.Entries[0] = []byte("tampered")
	b, err = json.Marshal(saved)
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(output, b, 0600); err != nil {
		t.Fatal(err)
	}
	if err := run([]string{"-offline", "-public-key", key, "-output", output}); err == nil {
		t.Fatal("accepted tampered snapshot")
	}
}
