package main

import (
	"fmt"
	"math"
)

// --- Matrix domain ---

type Matrix struct {
	Rows int
	Cols int
	Data [][]float64
}

func NewMatrix(rows, cols int) *Matrix {
	data := make([][]float64, rows)
	for i := range data {
		data[i] = make([]float64, cols)
	}
	return &Matrix{Rows: rows, Cols: cols, Data: data}
}

func FromSlice(rows int, cols int, values []float64) *Matrix {
	if len(values) != rows*cols {
		return nil
	}
	m := NewMatrix(rows, cols)
	for i := 0; i < rows; i++ {
		for j := 0; j < cols; j++ {
			m.Data[i][j] = values[i*cols+j]
		}
	}
	return m
}

func Identity(n int) *Matrix {
	m := NewMatrix(n, n)
	for i := 0; i < n; i++ {
		m.Data[i][i] = 1.0
	}
	return m
}

func (m *Matrix) Get(i, j int) float64 {
	return m.Data[i][j]
}

func (m *Matrix) Set(i, j int, v float64) {
	m.Data[i][j] = v
}

// --- Basic ops ---

func (m *Matrix) Add(other *Matrix) (*Matrix, error) {
	if m.Rows != other.Rows || m.Cols != other.Cols {
		return nil, fmt.Errorf("dimension mismatch: %dx%d vs %dx%d",
			m.Rows, m.Cols, other.Rows, other.Cols)
	}
	result := NewMatrix(m.Rows, m.Cols)
	for i := 0; i < m.Rows; i++ {
		for j := 0; j < m.Cols; j++ {
			result.Data[i][j] = m.Data[i][j] + other.Data[i][j]
		}
	}
	return result, nil
}

func (m *Matrix) Scale(factor float64) *Matrix {
	result := NewMatrix(m.Rows, m.Cols)
	for i := 0; i < m.Rows; i++ {
		for j := 0; j < m.Cols; j++ {
			result.Data[i][j] = m.Data[i][j] * factor
		}
	}
	return result
}

// --- Statistics ---

type Summary struct {
	Count  int
	Sum    float64
	Mean   float64
	Min    float64
	Max    float64
	StdDev float64
}

func Describe(values []float64) Summary {
	if len(values) == 0 {
		return Summary{}
	}
	s := Summary{
		Count: len(values),
		Min:   values[0],
		Max:   values[0],
	}
	for _, v := range values {
		s.Sum += v
		if v < s.Min {
			s.Min = v
		}
		if v > s.Max {
			s.Max = v
		}
	}
	s.Mean = s.Sum / float64(s.Count)

	var variance float64
	for _, v := range values {
		diff := v - s.Mean
		variance += diff * diff
	}
	variance /= float64(s.Count)
	s.StdDev = math.Sqrt(variance)
	return s
}

// --- Vector ops ---

func Dot(a, b []float64) (float64, error) {
	if len(a) != len(b) {
		return 0, fmt.Errorf("length mismatch: %d vs %d", len(a), len(b))
	}
	var result float64
	for i := range a {
		result += a[i] * b[i]
	}
	return result, nil
}

func Norm(v []float64) float64 {
	var sum float64
	for _, x := range v {
		sum += x * x
	}
	return math.Sqrt(sum)
}

// --- Stubs for challenge features ---

// Multiply should compute the matrix product m × other.
// Returns an error if dimensions are incompatible
// (m.Cols must equal other.Rows).
func (m *Matrix) Multiply(other *Matrix) (*Matrix, error) {
	panic("CHALLENGE: implement Multiply")
}

// Transpose should return a new matrix that is the transpose of m.
func (m *Matrix) Transpose() *Matrix {
	panic("CHALLENGE: implement Transpose")
}

// Determinant should compute the determinant of a square matrix using
// Laplace expansion (cofactor method). Returns an error if the matrix
// is not square.
func (m *Matrix) Determinant() (float64, error) {
	panic("CHALLENGE: implement Determinant")
}

// LinearRegression should compute the slope and intercept of the
// least-squares best-fit line for the given (x, y) data points.
// Returns slope, intercept, and an error if the input is invalid
// (empty or mismatched lengths).
func LinearRegression(xs, ys []float64) (slope, intercept float64, err error) {
	panic("CHALLENGE: implement LinearRegression")
}

// --- Main ---

func main() {
	m := Identity(3)
	fmt.Printf("Identity 3x3: %v\n", m.Data)

	a := FromSlice(2, 2, []float64{1, 2, 3, 4})
	b := FromSlice(2, 2, []float64{5, 6, 7, 8})
	sum, _ := a.Add(b)
	fmt.Printf("a + b = %v\n", sum.Data)

	values := []float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	stats := Describe(values)
	fmt.Printf("Mean: %.2f, StdDev: %.2f\n", stats.Mean, stats.StdDev)

	dot, _ := Dot([]float64{1, 2, 3}, []float64{4, 5, 6})
	fmt.Printf("Dot product: %.2f\n", dot)
}
