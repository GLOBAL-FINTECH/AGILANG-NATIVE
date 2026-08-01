//! Standalone native CPU computation and training primitives for AGILANG.
//!
//! This crate has no Python, NumPy, PyTorch, TensorFlow, JavaScript, or WASM
//! runtime dependency. Every operation executes as compiled Rust machine code.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

fn invalid(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message.into())
}

fn checked_element_count(shape: &[usize]) -> RuntimeResult<usize> {
    if shape.is_empty() {
        return Err(invalid("tensor shape cannot be empty"));
    }
    shape.iter().try_fold(1usize, |count, &dimension| {
        if dimension == 0 {
            return Err(invalid("tensor dimensions must be non-zero"));
        }
        count
            .checked_mul(dimension)
            .ok_or_else(|| AgilangError::new(ErrorCode::Overflow, "tensor element count overflow"))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tensor {
    shape: Vec<usize>,
    data: Vec<f64>,
}

impl Tensor {
    pub fn new(shape: Vec<usize>, data: Vec<f64>) -> RuntimeResult<Self> {
        let expected = checked_element_count(&shape)?;
        if expected != data.len() {
            return Err(invalid("tensor data length does not match shape"));
        }
        if data.iter().any(|value| !value.is_finite()) {
            return Err(invalid("tensor contains a non-finite value"));
        }
        Ok(Self { shape, data })
    }

    pub fn zeros(shape: Vec<usize>) -> RuntimeResult<Self> {
        let len = checked_element_count(&shape)?;
        Self::new(shape, vec![0.0; len])
    }

    pub fn ones(shape: Vec<usize>) -> RuntimeResult<Self> {
        let len = checked_element_count(&shape)?;
        Self::new(shape, vec![1.0; len])
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    pub fn rank(&self) -> usize {
        self.shape.len()
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn data(&self) -> &[f64] {
        &self.data
    }
    pub fn data_mut(&mut self) -> &mut [f64] {
        &mut self.data
    }

    pub fn get(&self, index: usize) -> RuntimeResult<f64> {
        self.data
            .get(index)
            .copied()
            .ok_or_else(|| invalid("tensor index out of bounds"))
    }

    pub fn set(&mut self, index: usize, value: f64) -> RuntimeResult<()> {
        if !value.is_finite() {
            return Err(invalid("tensor value must be finite"));
        }
        let slot = self
            .data
            .get_mut(index)
            .ok_or_else(|| invalid("tensor index out of bounds"))?;
        *slot = value;
        Ok(())
    }

    pub fn reshape(&self, shape: Vec<usize>) -> RuntimeResult<Self> {
        if checked_element_count(&shape)? != self.len() {
            return Err(invalid("reshape changes tensor element count"));
        }
        Self::new(shape, self.data.clone())
    }

    pub fn add(&self, rhs: &Self) -> RuntimeResult<Self> {
        self.same_shape(rhs)?;
        Self::new(
            self.shape.clone(),
            self.data
                .iter()
                .zip(&rhs.data)
                .map(|(a, b)| a + b)
                .collect(),
        )
    }

    pub fn sub(&self, rhs: &Self) -> RuntimeResult<Self> {
        self.same_shape(rhs)?;
        Self::new(
            self.shape.clone(),
            self.data
                .iter()
                .zip(&rhs.data)
                .map(|(a, b)| a - b)
                .collect(),
        )
    }

    pub fn hadamard(&self, rhs: &Self) -> RuntimeResult<Self> {
        self.same_shape(rhs)?;
        Self::new(
            self.shape.clone(),
            self.data
                .iter()
                .zip(&rhs.data)
                .map(|(a, b)| a * b)
                .collect(),
        )
    }

    pub fn scale(&self, factor: f64) -> RuntimeResult<Self> {
        if !factor.is_finite() {
            return Err(invalid("scale factor must be finite"));
        }
        Self::new(
            self.shape.clone(),
            self.data.iter().map(|v| v * factor).collect(),
        )
    }

    pub fn relu(&self) -> Self {
        Self {
            shape: self.shape.clone(),
            data: self.data.iter().map(|v| v.max(0.0)).collect(),
        }
    }

    pub fn sigmoid(&self) -> Self {
        Self {
            shape: self.shape.clone(),
            data: self.data.iter().map(|v| 1.0 / (1.0 + (-v).exp())).collect(),
        }
    }

    pub fn softmax(&self) -> RuntimeResult<Self> {
        if self.data.is_empty() {
            return Err(invalid("softmax requires a non-empty tensor"));
        }
        let max = self.data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut values: Vec<f64> = self.data.iter().map(|v| (v - max).exp()).collect();
        let sum: f64 = values.iter().sum();
        if !sum.is_finite() || sum == 0.0 {
            return Err(invalid("softmax normalization failed"));
        }
        for value in &mut values {
            *value /= sum;
        }
        Self::new(self.shape.clone(), values)
    }

    pub fn sum(&self) -> f64 {
        self.data.iter().sum()
    }
    pub fn mean(&self) -> f64 {
        self.sum() / self.data.len() as f64
    }

    pub fn transpose(&self) -> RuntimeResult<Self> {
        if self.shape.len() != 2 {
            return Err(invalid("transpose requires a rank-2 tensor"));
        }
        let (rows, cols) = (self.shape[0], self.shape[1]);
        let mut out = vec![0.0; self.len()];
        for row in 0..rows {
            for col in 0..cols {
                out[col * rows + row] = self.data[row * cols + col];
            }
        }
        Self::new(vec![cols, rows], out)
    }

    pub fn matmul(&self, rhs: &Self) -> RuntimeResult<Self> {
        if self.shape.len() != 2 || rhs.shape.len() != 2 {
            return Err(invalid("matmul requires rank-2 tensors"));
        }
        let (m, k) = (self.shape[0], self.shape[1]);
        let (rk, n) = (rhs.shape[0], rhs.shape[1]);
        if k != rk {
            return Err(invalid("matmul inner dimensions do not match"));
        }
        let mut out = vec![0.0; m * n];
        for i in 0..m {
            for p in 0..k {
                let a = self.data[i * k + p];
                for j in 0..n {
                    out[i * n + j] += a * rhs.data[p * n + j];
                }
            }
        }
        Self::new(vec![m, n], out)
    }

    pub fn mean_squared_error(&self, target: &Self) -> RuntimeResult<f64> {
        self.same_shape(target)?;
        let sum = self
            .data
            .iter()
            .zip(&target.data)
            .map(|(a, b)| {
                let d = a - b;
                d * d
            })
            .sum::<f64>();
        Ok(sum / self.data.len() as f64)
    }

    fn same_shape(&self, rhs: &Self) -> RuntimeResult<()> {
        if self.shape != rhs.shape {
            return Err(invalid("tensor shapes do not match"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Activation {
    Linear,
    Relu,
    Sigmoid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenseLayer {
    input_size: usize,
    output_size: usize,
    weights: Vec<f64>,
    bias: Vec<f64>,
    activation: Activation,
}

impl DenseLayer {
    pub fn new(
        input_size: usize,
        output_size: usize,
        activation: Activation,
    ) -> RuntimeResult<Self> {
        if input_size == 0 || output_size == 0 {
            return Err(invalid("dense dimensions must be non-zero"));
        }
        let scale = (2.0 / input_size as f64).sqrt();
        let weights = (0..input_size * output_size)
            .map(|index| {
                (((index.wrapping_mul(1103515245).wrapping_add(12345)) % 65536) as f64 / 32768.0
                    - 1.0)
                    * scale
            })
            .collect();
        Ok(Self {
            input_size,
            output_size,
            weights,
            bias: vec![0.0; output_size],
            activation,
        })
    }

    pub fn forward(&self, input: &[f64]) -> RuntimeResult<Vec<f64>> {
        if input.len() != self.input_size {
            return Err(invalid("dense input size mismatch"));
        }
        let mut output = self.bias.clone();
        for (out_index, out) in output.iter_mut().enumerate() {
            for (in_index, value) in input.iter().enumerate() {
                *out += value * self.weights[in_index * self.output_size + out_index];
            }
            *out = match self.activation {
                Activation::Linear => *out,
                Activation::Relu => out.max(0.0),
                Activation::Sigmoid => 1.0 / (1.0 + (-*out).exp()),
            };
        }
        Ok(output)
    }
}

/// A standalone trainable linear model using native f64 machine operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearModel {
    pub weights: Vec<f64>,
    pub bias: f64,
}

impl LinearModel {
    pub fn new(feature_count: usize) -> RuntimeResult<Self> {
        if feature_count == 0 {
            return Err(invalid("feature count must be non-zero"));
        }
        Ok(Self {
            weights: vec![0.0; feature_count],
            bias: 0.0,
        })
    }

    pub fn predict(&self, features: &[f64]) -> RuntimeResult<f64> {
        if features.len() != self.weights.len() {
            return Err(invalid("feature count mismatch"));
        }
        Ok(self
            .weights
            .iter()
            .zip(features)
            .map(|(w, x)| w * x)
            .sum::<f64>()
            + self.bias)
    }

    pub fn train_batch(
        &mut self,
        features: &Tensor,
        targets: &[f64],
        learning_rate: f64,
    ) -> RuntimeResult<f64> {
        if features.rank() != 2 {
            return Err(invalid("training features must be rank-2"));
        }
        let rows = features.shape()[0];
        let cols = features.shape()[1];
        if cols != self.weights.len() || rows != targets.len() || rows == 0 {
            return Err(invalid("training batch dimensions do not match model"));
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(invalid("learning rate must be positive and finite"));
        }
        let mut gradient = vec![0.0; cols];
        let mut bias_gradient = 0.0;
        let mut loss = 0.0;
        for (row, target) in targets.iter().enumerate() {
            let sample = &features.data[row * cols..(row + 1) * cols];
            let error = self.predict(sample)? - target;
            loss += error * error;
            for col in 0..cols {
                gradient[col] += 2.0 * error * sample[col] / rows as f64;
            }
            bias_gradient += 2.0 * error / rows as f64;
        }
        for (weight, grad) in self.weights.iter_mut().zip(gradient) {
            *weight -= learning_rate * grad;
        }
        self.bias -= learning_rate * bias_gradient;
        Ok(loss / rows as f64)
    }
}

/// One native SGD update for y = x*w + b using mean-squared error.
pub fn linear_regression_step(
    x: &[f64],
    y: &[f64],
    weight: &mut f64,
    bias: &mut f64,
    learning_rate: f64,
) -> RuntimeResult<f64> {
    let features = Tensor::new(vec![x.len(), 1], x.to_vec())?;
    let mut model = LinearModel {
        weights: vec![*weight],
        bias: *bias,
    };
    let loss = model.train_batch(&features, y, learning_rate)?;
    *weight = model.weights[0];
    *bias = model.bias;
    Ok(loss)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ComputeCapabilities {
    pub backend: &'static str,
    pub native: bool,
    pub python_required: bool,
    pub wasm_required: bool,
    pub logical_cpus: usize,
    pub cpu_features: Vec<&'static str>,
}

pub fn capabilities() -> ComputeCapabilities {
    ComputeCapabilities {
        backend: "native-cpu",
        native: true,
        python_required: false,
        wasm_required: false,
        logical_cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        cpu_features: cpu_features(),
    }
}

pub fn cpu_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("sse2") {
            features.push("sse2");
        }
        if std::is_x86_feature_detected!("avx") {
            features.push("avx");
        }
        if std::is_x86_feature_detected!("avx2") {
            features.push("avx2");
        }
        if std::is_x86_feature_detected!("fma") {
            features.push("fma");
        }
    }
    features
}

/// Reverse-mode automatic-differentiation node identifier.
pub type NodeId = usize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TapeOp {
    Leaf,
    Add(NodeId, NodeId),
    Mul(NodeId, NodeId),
    Relu(NodeId),
    Mean(NodeId),
    MatMul(NodeId, NodeId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapeNode {
    pub value: Tensor,
    pub gradient: Option<Tensor>,
    pub op: TapeOp,
    pub requires_grad: bool,
}

/// Small native reverse-mode autograd tape. It is deliberately deterministic
/// and CPU-only; accelerator providers may implement the same operation set.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AutogradTape {
    nodes: Vec<TapeNode>,
}

impl AutogradTape {
    pub fn variable(&mut self, value: Tensor, requires_grad: bool) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(TapeNode {
            value,
            gradient: None,
            op: TapeOp::Leaf,
            requires_grad,
        });
        id
    }
    pub fn value(&self, id: NodeId) -> RuntimeResult<&Tensor> {
        self.nodes
            .get(id)
            .map(|n| &n.value)
            .ok_or_else(|| invalid("unknown autograd node"))
    }
    pub fn gradient(&self, id: NodeId) -> RuntimeResult<Option<&Tensor>> {
        Ok(self
            .nodes
            .get(id)
            .ok_or_else(|| invalid("unknown autograd node"))?
            .gradient
            .as_ref())
    }
    fn push(&mut self, value: Tensor, op: TapeOp, requires_grad: bool) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(TapeNode {
            value,
            gradient: None,
            op,
            requires_grad,
        });
        id
    }
    pub fn add(&mut self, a: NodeId, b: NodeId) -> RuntimeResult<NodeId> {
        let v = self.value(a)?.add(self.value(b)?)?;
        Ok(self.push(
            v,
            TapeOp::Add(a, b),
            self.nodes[a].requires_grad || self.nodes[b].requires_grad,
        ))
    }
    pub fn mul(&mut self, a: NodeId, b: NodeId) -> RuntimeResult<NodeId> {
        let v = self.value(a)?.hadamard(self.value(b)?)?;
        Ok(self.push(
            v,
            TapeOp::Mul(a, b),
            self.nodes[a].requires_grad || self.nodes[b].requires_grad,
        ))
    }
    pub fn relu(&mut self, a: NodeId) -> RuntimeResult<NodeId> {
        let v = self.value(a)?.relu();
        Ok(self.push(v, TapeOp::Relu(a), self.nodes[a].requires_grad))
    }
    pub fn mean(&mut self, a: NodeId) -> RuntimeResult<NodeId> {
        let v = Tensor::new(vec![1], vec![self.value(a)?.mean()])?;
        Ok(self.push(v, TapeOp::Mean(a), self.nodes[a].requires_grad))
    }
    pub fn matmul(&mut self, a: NodeId, b: NodeId) -> RuntimeResult<NodeId> {
        let v = self.value(a)?.matmul(self.value(b)?)?;
        Ok(self.push(
            v,
            TapeOp::MatMul(a, b),
            self.nodes[a].requires_grad || self.nodes[b].requires_grad,
        ))
    }
    fn accumulate(&mut self, id: NodeId, g: Tensor) -> RuntimeResult<()> {
        if !self.nodes[id].requires_grad {
            return Ok(());
        }
        self.nodes[id].gradient = Some(match self.nodes[id].gradient.take() {
            Some(old) => old.add(&g)?,
            None => g,
        });
        Ok(())
    }
    pub fn backward(&mut self, loss: NodeId) -> RuntimeResult<()> {
        if self.value(loss)?.len() != 1 {
            return Err(invalid("backward requires a scalar loss"));
        }
        self.nodes[loss].gradient = Some(Tensor::ones(vec![1])?);
        for id in (0..=loss).rev() {
            let Some(g) = self.nodes[id].gradient.clone() else {
                continue;
            };
            match self.nodes[id].op {
                TapeOp::Leaf => {}
                TapeOp::Add(a, b) => {
                    self.accumulate(a, g.clone())?;
                    self.accumulate(b, g)?;
                }
                TapeOp::Mul(a, b) => {
                    let ga = g.hadamard(self.value(b)?)?;
                    let gb = g.hadamard(self.value(a)?)?;
                    self.accumulate(a, ga)?;
                    self.accumulate(b, gb)?;
                }
                TapeOp::Relu(a) => {
                    let mask = Tensor::new(
                        self.value(a)?.shape().to_vec(),
                        self.value(a)?
                            .data()
                            .iter()
                            .map(|v| if *v > 0.0 { 1.0 } else { 0.0 })
                            .collect(),
                    )?;
                    self.accumulate(a, g.hadamard(&mask)?)?;
                }
                TapeOp::Mean(a) => {
                    let n = self.value(a)?.len();
                    let expanded = Tensor::new(
                        self.value(a)?.shape().to_vec(),
                        vec![g.data()[0] / n as f64; n],
                    )?;
                    self.accumulate(a, expanded)?;
                }
                TapeOp::MatMul(a, b) => {
                    let ga = g.matmul(&self.value(b)?.transpose()?)?;
                    let gb = self.value(a)?.transpose()?.matmul(&g)?;
                    self.accumulate(a, ga)?;
                    self.accumulate(b, gb)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdamState {
    pub step: u64,
    pub first: Vec<f64>,
    pub second: Vec<f64>,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
}
impl AdamState {
    pub fn new(parameter_count: usize) -> Self {
        Self {
            step: 0,
            first: vec![0.0; parameter_count],
            second: vec![0.0; parameter_count],
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
        }
    }
    pub fn update(
        &mut self,
        parameters: &mut [f64],
        gradients: &[f64],
        learning_rate: f64,
    ) -> RuntimeResult<()> {
        if parameters.len() != gradients.len() || parameters.len() != self.first.len() {
            return Err(invalid("Adam parameter shape mismatch"));
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(invalid("Adam learning rate must be positive"));
        }
        self.step += 1;
        let t = self.step as i32;
        for i in 0..parameters.len() {
            let g = gradients[i];
            if !g.is_finite() {
                return Err(invalid("non-finite gradient"));
            }
            self.first[i] = self.beta1 * self.first[i] + (1.0 - self.beta1) * g;
            self.second[i] = self.beta2 * self.second[i] + (1.0 - self.beta2) * g * g;
            let mh = self.first[i] / (1.0 - self.beta1.powi(t));
            let vh = self.second[i] / (1.0 - self.beta2.powi(t));
            parameters[i] -= learning_rate * mh / (vh.sqrt() + self.epsilon);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_operations_are_correct() {
        let a = Tensor::new(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let b = Tensor::new(vec![2, 1], vec![5.0, 6.0]).unwrap();
        assert_eq!(a.matmul(&b).unwrap().data(), &[17.0, 39.0]);
        assert_eq!(a.transpose().unwrap().data(), &[1.0, 3.0, 2.0, 4.0]);
    }

    #[test]
    fn softmax_is_normalized() {
        let values = Tensor::new(vec![3], vec![1.0, 2.0, 3.0])
            .unwrap()
            .softmax()
            .unwrap();
        assert!((values.sum() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn native_model_training_reduces_loss() {
        let features = Tensor::new(vec![4, 1], vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let targets = [3.0, 5.0, 7.0, 9.0];
        let mut model = LinearModel::new(1).unwrap();
        let first = model.train_batch(&features, &targets, 0.01).unwrap();
        let mut last = first;
        for _ in 0..1000 {
            last = model.train_batch(&features, &targets, 0.01).unwrap();
        }
        assert!(last < first);
        assert!((model.weights[0] - 2.0).abs() < 0.1);
        assert!((model.bias - 1.0).abs() < 0.2);
    }

    #[test]
    fn autograd_computes_basic_gradient() {
        let mut tape = AutogradTape::default();
        let x = tape.variable(Tensor::new(vec![2], vec![2.0, 3.0]).unwrap(), true);
        let y = tape.mul(x, x).unwrap();
        let loss = tape.mean(y).unwrap();
        tape.backward(loss).unwrap();
        assert_eq!(tape.gradient(x).unwrap().unwrap().data(), &[2.0, 3.0]);
    }

    #[test]
    fn reports_zero_python_native_capabilities() {
        let caps = capabilities();
        assert!(caps.native);
        assert!(!caps.python_required);
        assert!(!caps.wasm_required);
    }
}
