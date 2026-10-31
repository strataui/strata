//! Pure-Rust Modified Nodal Analysis (MNA) transient circuit simulation engine.

/// Component in an electrical circuit netlist.
#[derive(Debug, Clone)]
pub enum CircuitElement {
    Resistor {
        node_a: usize,
        node_b: usize,
        resistance: f64,
    },
    Capacitor {
        node_a: usize,
        node_b: usize,
        capacitance: f64,
        prev_voltage: f64,
        prev_current: f64,
    },
    VoltageSource {
        node_pos: usize,
        node_neg: usize,
        voltage: f64,
    },
    Inverter {
        node_in: usize,
        node_out: usize,
        vdd: f64,
    },
}

/// Transient simulation state and Modified Nodal Analysis solver.
#[derive(Debug, Clone)]
pub struct SpiceCircuit {
    pub num_nodes: usize,
    pub elements: Vec<CircuitElement>,
    pub time_step: f64,
    pub current_time: f64,
    pub node_voltages: Vec<f64>,
}

impl SpiceCircuit {
    pub fn new(num_nodes: usize, time_step: f64) -> Self {
        Self {
            num_nodes,
            elements: Vec::new(),
            time_step,
            current_time: 0.0,
            node_voltages: vec![0.0; num_nodes + 1],
        }
    }

    pub fn add_element(&mut self, elem: CircuitElement) {
        self.elements.push(elem);
    }

    /// Constructs a 3-stage CMOS Ring Oscillator benchmark circuit.
    pub fn create_ring_oscillator(vdd: f64, time_step: f64) -> Self {
        let mut ckt = Self::new(3, time_step);
        // Stage 1: Inverter Node 1 -> Node 2
        ckt.add_element(CircuitElement::Inverter { node_in: 1, node_out: 2, vdd });
        ckt.add_element(CircuitElement::Capacitor { node_a: 2, node_b: 0, capacitance: 1e-12, prev_voltage: 0.0, prev_current: 0.0 });

        // Stage 2: Inverter Node 2 -> Node 3
        ckt.add_element(CircuitElement::Inverter { node_in: 2, node_out: 3, vdd });
        ckt.add_element(CircuitElement::Capacitor { node_a: 3, node_b: 0, capacitance: 1e-12, prev_voltage: 0.0, prev_current: 0.0 });

        // Stage 3: Inverter Node 3 -> Node 1 (Feedback Loop)
        ckt.add_element(CircuitElement::Inverter { node_in: 3, node_out: 1, vdd });
        ckt.add_element(CircuitElement::Capacitor { node_a: 1, node_b: 0, capacitance: 1e-12, prev_voltage: 0.1, prev_current: 0.0 }); // Slight seed asymmetry to start oscillation

        ckt
    }

    /// Constructs an RLC resonant circuit benchmark.
    pub fn create_rlc_circuit(r: f64, c: f64, time_step: f64) -> Self {
        let mut ckt = Self::new(2, time_step);
        ckt.add_element(CircuitElement::VoltageSource { node_pos: 1, node_neg: 0, voltage: 5.0 });
        ckt.add_element(CircuitElement::Resistor { node_a: 1, node_b: 2, resistance: r });
        ckt.add_element(CircuitElement::Capacitor { node_a: 2, node_b: 0, capacitance: c, prev_voltage: 0.0, prev_current: 0.0 });
        ckt
    }

    /// Advances transient simulation by one time step h.
    pub fn step(&mut self) {
        let h = self.time_step;
        let n = self.num_nodes;
        let mut g_matrix = vec![vec![0.0; n + 1]; n + 1];
        let mut i_vector = vec![0.0; n + 1];

        for elem in &mut self.elements {
            match elem {
                CircuitElement::Resistor { node_a, node_b, resistance } => {
                    let conductance = 1.0 / *resistance;
                    Self::stamp_conductance(&mut g_matrix, *node_a, *node_b, conductance);
                }
                CircuitElement::Capacitor { node_a, node_b, capacitance, prev_voltage, prev_current } => {
                    // Trapezoidal companion model: Geq = 2C / h, Ieq = Geq * v_n + i_n
                    let g_eq = (2.0 * *capacitance) / h;
                    let i_eq = g_eq * *prev_voltage + *prev_current;
                    Self::stamp_conductance(&mut g_matrix, *node_a, *node_b, g_eq);

                    if *node_a != 0 {
                        i_vector[*node_a] += i_eq;
                    }
                    if *node_b != 0 {
                        i_vector[*node_b] -= i_eq;
                    }
                }
                CircuitElement::VoltageSource { node_pos, node_neg, voltage } => {
                    // Fixed potential constraint
                    if *node_pos != 0 {
                        g_matrix[*node_pos][*node_pos] += 1e6;
                        i_vector[*node_pos] += *voltage * 1e6;
                    }
                    if *node_neg != 0 {
                        g_matrix[*node_neg][*node_neg] += 1e6;
                    }
                }
                CircuitElement::Inverter { node_in, node_out, vdd } => {
                    // Nonlinear CMOS transfer curve: Vout = Vdd / (1 + exp((Vin - Vdd/2) / (0.1 Vdd)))
                    let v_in = self.node_voltages.get(*node_in).copied().unwrap_or(0.0);
                    let v_mid = *vdd * 0.5;
                    let exponent = ((v_in - v_mid) / (0.08 * *vdd)).clamp(-20.0, 20.0);
                    let v_target = *vdd / (1.0 + exponent.exp());

                    // Virtual driver with output resistance Rout = 50 ohms
                    let g_out = 1.0 / 50.0;
                    Self::stamp_conductance(&mut g_matrix, *node_out, 0, g_out);
                    if *node_out != 0 {
                        i_vector[*node_out] += v_target * g_out;
                    }
                }
            }
        }

        // Solve G * V = I using Gaussian elimination for active nodes 1..=n
        let solved = Self::solve_linear_system(&g_matrix, &i_vector, n);
        for i in 1..=n {
            self.node_voltages[i] = solved[i];
        }

        // Update dynamic element histories
        for elem in &mut self.elements {
            if let CircuitElement::Capacitor { node_a, node_b, capacitance, prev_voltage, prev_current } = elem {
                let v_a = self.node_voltages.get(*node_a).copied().unwrap_or(0.0);
                let v_b = self.node_voltages.get(*node_b).copied().unwrap_or(0.0);
                let v_curr = v_a - v_b;
                let g_eq = (2.0 * *capacitance) / h;
                let i_curr = g_eq * (v_curr - *prev_voltage) - *prev_current;

                *prev_voltage = v_curr;
                *prev_current = i_curr;
            }
        }

        self.current_time += h;
    }

    fn stamp_conductance(g: &mut [Vec<f64>], a: usize, b: usize, val: f64) {
        if a != 0 {
            g[a][a] += val;
        }
        if b != 0 {
            g[b][b] += val;
        }
        if a != 0 && b != 0 {
            g[a][b] -= val;
            g[b][a] -= val;
        }
    }

    fn solve_linear_system(g: &[Vec<f64>], i_vec: &[f64], n: usize) -> Vec<f64> {
        let mut a = vec![vec![0.0; n + 1]; n + 1];
        let mut b = vec![0.0; n + 1];
        for r in 1..=n {
            for c in 1..=n {
                a[r][c] = g[r][c];
            }
            b[r] = i_vec[r];
        }

        // Forward Gaussian elimination with partial pivoting
        for k in 1..=n {
            let mut max_row = k;
            let mut max_val = a[k][k].abs();
            for r in (k + 1)..=n {
                if a[r][k].abs() > max_val {
                    max_val = a[r][k].abs();
                    max_row = r;
                }
            }
            if max_val < 1e-12 {
                continue; // Singular / floating node
            }
            if max_row != k {
                a.swap(k, max_row);
                b.swap(k, max_row);
            }

            for r in (k + 1)..=n {
                let factor = a[r][k] / a[k][k];
                for c in k..=n {
                    a[r][c] -= factor * a[k][c];
                }
                b[r] -= factor * b[k];
            }
        }

        // Back substitution
        let mut x = vec![0.0; n + 1];
        for r in (1..=n).rev() {
            if a[r][r].abs() > 1e-12 {
                let mut sum = b[r];
                for c in (r + 1)..=n {
                    sum -= a[r][c] * x[c];
                }
                x[r] = sum / a[r][r];
            }
        }

        x
    }
}
