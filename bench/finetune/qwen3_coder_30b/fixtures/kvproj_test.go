package nebulatest

import (
	"testing"
	"time"
)

func newTestCache() *KVCache {
	return NewKVCache(100, NewLogger("test"), NewMetrics())
}

func TestKVCache_SetGet(t *testing.T) {
	c := newTestCache()
	c.Set("foo", "bar")
	got, ok := c.Get("foo")
	if !ok || got != "bar" {
		t.Fatalf("Get(foo) = %q, %v; want bar, true", got, ok)
	}
}

func TestKVCache_GetMissing(t *testing.T) {
	c := newTestCache()
	if _, ok := c.Get("nope"); ok {
		t.Fatal("expected missing key")
	}
}

func TestKVCache_Delete(t *testing.T) {
	c := newTestCache()
	c.Set("k", "v")
	if !c.Delete("k") {
		t.Fatal("expected delete to succeed")
	}
	if _, ok := c.Get("k"); ok {
		t.Fatal("key still present after delete")
	}
}

func TestKVCache_TTLExpiry(t *testing.T) {
	c := newTestCache()
	c.SetTTL("temp", "val", 10*time.Millisecond)
	time.Sleep(20 * time.Millisecond)
	if _, ok := c.Get("temp"); ok {
		t.Fatal("expected ttl expiry")
	}
}

func TestKVCache_Len(t *testing.T) {
	c := newTestCache()
	for i := 0; i < 5; i++ {
		c.Set(string(rune('a'+i)), "x")
	}
	if c.Len() != 5 {
		t.Fatalf("Len = %d, want 5", c.Len())
	}
}
