package main

import (
	"math"
	"testing"
)

func eq(a, b float64) bool {
	return math.Abs(a-b) < 1e-9
}

// --- Existing tests ---

func TestNewMatrix(t *testing.T) {
	m := NewMatrix(3, 4)
	if m.Rows != 3 || m.Cols != 4 {
		t.Fatal("dimensions wrong")
	}
	if m.Data[0][0] != 0 {
		t.Fatal("default value should be 0")
	}
}

func TestIdentity(t *testing.T) {
	m := Identity(3)
	for i := 0; i < 3; i++ {
		for j := 0; j < 3; j++ {
			expected := 0.0
			if i == j {
				expected = 1.0
			}
			if m.Data[i][j] != expected {
				t.Fatalf("Identity[%d][%d] wrong", i, j)
			}
		}
	}
}

func TestAdd(t *testing.T) {
	a := FromSlice(2, 2, []float64{1, 2, 3, 4})
	b := FromSlice(2, 2, []float64{5, 6, 7, 8})
	sum, err := a.Add(b)
	if err != nil {
		t.Fatal(err)
	}
	expected := []float64{6, 8, 10, 12}
	for i := 0; i < 4; i++ {
		if sum.Data[i/2][i%2] != expected[i] {
			t.Fatalf("Add wrong at %d", i)
		}
	}
}

func TestScale(t *testing.T) {
	m := FromSlice(2, 2, []float64{1, 2, 3, 4})
	scaled := m.Scale(2)
	if scaled.Data[0][0] != 2 || scaled.Data[1][1] != 8 {
		t.Fatal("Scale wrong")
	}
}

func TestDescribe(t *testing.T) {
	s := Describe([]float64{1, 2, 3, 4, 5})
	if s.Count != 5 {
		t.Fatal("count wrong")
	}
	if !eq(s.Mean, 3.0) {
		t.Fatalf("mean wrong: %v", s.Mean)
	}
	if s.Min != 1 || s.Max != 5 {
		t.Fatal("min/max wrong")
	}
}

func TestDot(t *testing.T) {
	d, err := Dot([]float64{1, 2, 3}, []float64{4, 5, 6})
	if err != nil {
		t.Fatal(err)
	}
	// 1*4 + 2*5 + 3*6 = 32
	if !eq(d, 32.0) {
		t.Fatalf("dot product wrong: %v", d)
	}
}

func TestNorm(t *testing.T) {
	n := Norm([]float64{3, 4})
	if !eq(n, 5.0) {
		t.Fatalf("norm wrong: %v", n)
	}
}

// --- CHALLENGE TESTS ---

func TestChallenge_Multiply(t *testing.T) {
	t.Skip("CHALLENGE: Implement Multiply")

	a := FromSlice(2, 3, []float64{1, 2, 3, 4, 5, 6})
	b := FromSlice(3, 2, []float64{7, 8, 9, 10, 11, 12})

	result, err := a.Multiply(b)
	if err != nil {
		t.Fatal(err)
	}
	if result.Rows != 2 || result.Cols != 2 {
		t.Fatalf("result dimensions wrong: %dx%d", result.Rows, result.Cols)
	}
	// a*b:
	// [1*7+2*9+3*11, 1*8+2*10+3*12]   = [58, 64]
	// [4*7+5*9+6*11, 4*8+5*10+6*12]   = [139, 154]
	if !eq(result.Data[0][0], 58) || !eq(result.Data[0][1], 64) {
		t.Fatalf("row 0 wrong: %v", result.Data[0])
	}
	if !eq(result.Data[1][0], 139) || !eq(result.Data[1][1], 154) {
		t.Fatalf("row 1 wrong: %v", result.Data[1])
	}

	// Incompatible
	bad := FromSlice(2, 2, []float64{1, 2, 3, 4})
	if _, err := a.Multiply(bad); err == nil {
		t.Fatal("expected dimension error")
	}
}

func TestChallenge_Transpose(t *testing.T) {
	t.Skip("CHALLENGE: Implement Transpose")

	m := FromSlice(2, 3, []float64{1, 2, 3, 4, 5, 6})
	t2 := m.Transpose()
	if t2.Rows != 3 || t2.Cols != 2 {
		t.Fatalf("transpose dimensions wrong: %dx%d", t2.Rows, t2.Cols)
	}
	if t2.Data[0][0] != 1 || t2.Data[0][1] != 4 {
		t.Fatal("transpose[0] wrong")
	}
	if t2.Data[2][1] != 6 {
		t.Fatal("transpose[2][1] wrong")
	}
}

func TestChallenge_Determinant(t *testing.T) {
	t.Skip("CHALLENGE: Implement Determinant")

	// 2x2: det = ad - bc
	a := FromSlice(2, 2, []float64{4, 3, 6, 3})
	d, err := a.Determinant()
	if err != nil {
		t.Fatal(err)
	}
	// 4*3 - 3*6 = 12 - 18 = -6
	if !eq(d, -6) {
		t.Fatalf("expected -6, got %v", d)
	}

	// Identity has det 1
	i3 := Identity(3)
	d2, _ := i3.Determinant()
	if !eq(d2, 1.0) {
		t.Fatalf("identity det should be 1, got %v", d2)
	}

	// Non-square
	rect := NewMatrix(2, 3)
	if _, err := rect.Determinant(); err == nil {
		t.Fatal("expected error for non-square matrix")
	}
}

func TestChallenge_LinearRegression(t *testing.T) {
	t.Skip("CHALLENGE: Implement LinearRegression")

	// Perfect line: y = 2x + 1
	xs := []float64{1, 2, 3, 4, 5}
	ys := []float64{3, 5, 7, 9, 11}

	slope, intercept, err := LinearRegression(xs, ys)
	if err != nil {
		t.Fatal(err)
	}
	if !eq(slope, 2.0) {
		t.Fatalf("slope wrong: %v (expected 2)", slope)
	}
	if !eq(intercept, 1.0) {
		t.Fatalf("intercept wrong: %v (expected 1)", intercept)
	}

	// Empty input
	_, _, err = LinearRegression([]float64{}, []float64{})
	if err == nil {
		t.Fatal("expected error for empty input")
	}

	// Mismatched
	_, _, err = LinearRegression([]float64{1, 2}, []float64{1})
	if err == nil {
		t.Fatal("expected error for mismatched lengths")
	}
}
