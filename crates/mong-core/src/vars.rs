//! Kho biến của cốt truyện với ngữ nghĩa xác định cho mọi phép toán.

use crate::ir::{BinOp, Cond, CondOp, Effect, Expr, SetOp, Value};
use crate::vm::VmError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Kho biến. Dùng BTreeMap để mọi phép duyệt/serialize đều có thứ tự ổn định
/// (điều kiện cần cho golden test và save file so sánh được).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VarStore(BTreeMap<String, Value>);

impl From<BTreeMap<String, Value>> for VarStore {
    fn from(m: BTreeMap<String, Value>) -> Self {
        VarStore(m)
    }
}

impl VarStore {
    /// Ghi đè trực tiếp (cho `rand`/`set_expr` — kiểu đã được kiểm ở nơi gọi).
    pub fn set(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_string(), value);
    }

    /// Đánh giá biểu thức của `set_expr` (spec-ir v1). Số học Int-only;
    /// biến chưa tồn tại đọc ra 0 (nhất quán add/sub); tràn thì bão hoà.
    /// `target` là biến đích của phép gán, chỉ dùng cho thông điệp lỗi.
    pub fn eval_expr(&self, expr: &Expr, target: &str) -> Result<Value, VmError> {
        match expr {
            Expr::Lit(v) => Ok(v.clone()),
            Expr::Var(name) => Ok(self.0.get(name).cloned().unwrap_or(Value::Int(0))),
            Expr::Neg(e) => match self.eval_expr(e, target)? {
                Value::Int(n) => Ok(Value::Int(n.saturating_neg())),
                _ => Err(VmError::TypeMismatch {
                    var: target.to_string(),
                }),
            },
            Expr::Bin { op, lhs, rhs } => {
                let (a, b) = (self.eval_expr(lhs, target)?, self.eval_expr(rhs, target)?);
                let (Value::Int(a), Value::Int(b)) = (a, b) else {
                    return Err(VmError::TypeMismatch {
                        var: target.to_string(),
                    });
                };
                Ok(Value::Int(match op {
                    BinOp::Add => a.saturating_add(b),
                    BinOp::Sub => a.saturating_sub(b),
                    BinOp::Mul => a.saturating_mul(b),
                    BinOp::Div if b == 0 => {
                        return Err(VmError::DivByZero {
                            var: target.to_string(),
                        })
                    }
                    // saturating: i64::MIN / -1 không panic, ra i64::MAX
                    BinOp::Div => a.saturating_div(b),
                    BinOp::Rem if b == 0 => {
                        return Err(VmError::DivByZero {
                            var: target.to_string(),
                        })
                    }
                    // wrapping: i64::MIN % -1 không panic, ra 0 (đúng toán học)
                    BinOp::Rem => a.wrapping_rem(b),
                }))
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Áp một phép ghi biến. Ngữ nghĩa (xem spec-ir.md, lệnh `set`):
    /// - `assign`: ghi đè, tạo biến nếu chưa có.
    /// - `add`/`sub`: chỉ trên Int; biến chưa có coi là 0; tràn số thì bão hoà.
    /// - `toggle`: chỉ trên Bool; biến chưa có coi là false.
    /// - Sai kiểu → `VmError::TypeMismatch`, KHÔNG ghi gì.
    pub fn apply(&mut self, e: &Effect) -> Result<(), VmError> {
        match e.op {
            SetOp::Assign => {
                self.0.insert(e.var.clone(), e.value.clone());
                Ok(())
            }
            SetOp::Add | SetOp::Sub => {
                let delta = match &e.value {
                    Value::Int(i) => *i,
                    _ => return Err(VmError::TypeMismatch { var: e.var.clone() }),
                };
                let cur = match self.0.get(&e.var) {
                    Some(Value::Int(i)) => *i,
                    None => 0,
                    Some(_) => return Err(VmError::TypeMismatch { var: e.var.clone() }),
                };
                let next = if e.op == SetOp::Add {
                    cur.saturating_add(delta)
                } else {
                    cur.saturating_sub(delta)
                };
                self.0.insert(e.var.clone(), Value::Int(next));
                Ok(())
            }
            SetOp::Toggle => {
                let cur = match self.0.get(&e.var) {
                    Some(Value::Bool(b)) => *b,
                    None => false,
                    Some(_) => return Err(VmError::TypeMismatch { var: e.var.clone() }),
                };
                self.0.insert(e.var.clone(), Value::Bool(!cur));
                Ok(())
            }
        }
    }

    /// Đánh giá điều kiện. Biến chưa có nhận giá trị mặc định theo kiểu vế phải
    /// (Int→0, Bool→false, Str→""). So sánh khác kiểu → `TypeMismatch`.
    /// Ge/Le chỉ hợp lệ trên Int.
    pub fn eval(&self, c: &Cond) -> Result<bool, VmError> {
        let lhs = self.0.get(&c.var).cloned().unwrap_or(match &c.value {
            Value::Int(_) => Value::Int(0),
            Value::Bool(_) => Value::Bool(false),
            Value::Str(_) => Value::Str(String::new()),
        });
        match (&lhs, &c.value) {
            (Value::Int(a), Value::Int(b)) => Ok(match c.op {
                CondOp::Ge => a >= b,
                CondOp::Le => a <= b,
                CondOp::Eq => a == b,
                CondOp::Ne => a != b,
            }),
            (Value::Bool(a), Value::Bool(b)) => match c.op {
                CondOp::Eq => Ok(a == b),
                CondOp::Ne => Ok(a != b),
                _ => Err(VmError::TypeMismatch { var: c.var.clone() }),
            },
            (Value::Str(a), Value::Str(b)) => match c.op {
                CondOp::Eq => Ok(a == b),
                CondOp::Ne => Ok(a != b),
                _ => Err(VmError::TypeMismatch { var: c.var.clone() }),
            },
            _ => Err(VmError::TypeMismatch { var: c.var.clone() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eff(var: &str, op: SetOp, v: Value) -> Effect {
        Effect {
            var: var.into(),
            op,
            value: v,
        }
    }

    #[test]
    fn add_tren_bien_chua_co_coi_la_khong() {
        let mut vs = VarStore::default();
        vs.apply(&eff("tc", SetOp::Add, Value::Int(2))).unwrap();
        assert_eq!(vs.get("tc"), Some(&Value::Int(2)));
    }

    #[test]
    fn sub_va_bao_hoa() {
        let mut vs = VarStore::default();
        vs.apply(&eff("x", SetOp::Assign, Value::Int(i64::MIN)))
            .unwrap();
        vs.apply(&eff("x", SetOp::Sub, Value::Int(1))).unwrap();
        assert_eq!(vs.get("x"), Some(&Value::Int(i64::MIN)));
    }

    #[test]
    fn toggle_bool() {
        let mut vs = VarStore::default();
        vs.apply(&eff("flag", SetOp::Toggle, Value::Bool(true)))
            .unwrap();
        assert_eq!(vs.get("flag"), Some(&Value::Bool(true)));
        vs.apply(&eff("flag", SetOp::Toggle, Value::Bool(true)))
            .unwrap();
        assert_eq!(vs.get("flag"), Some(&Value::Bool(false)));
    }

    #[test]
    fn sai_kieu_khong_ghi() {
        let mut vs = VarStore::default();
        vs.apply(&eff("s", SetOp::Assign, Value::Str("a".into())))
            .unwrap();
        assert!(vs.apply(&eff("s", SetOp::Add, Value::Int(1))).is_err());
        assert_eq!(vs.get("s"), Some(&Value::Str("a".into())));
    }

    #[test]
    fn eval_mac_dinh_theo_kieu() {
        let vs = VarStore::default();
        let c = Cond {
            var: "tc".into(),
            op: CondOp::Ge,
            value: Value::Int(1),
        };
        assert!(!vs.eval(&c).unwrap());
        let c2 = Cond {
            var: "tc".into(),
            op: CondOp::Le,
            value: Value::Int(0),
        };
        assert!(vs.eval(&c2).unwrap());
    }

    #[test]
    fn eval_ge_tren_bool_la_loi() {
        let vs = VarStore::default();
        let c = Cond {
            var: "f".into(),
            op: CondOp::Ge,
            value: Value::Bool(true),
        };
        assert!(vs.eval(&c).is_err());
    }
}
