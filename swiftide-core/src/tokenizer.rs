// TODO: Naming
//
/// Enables a type to estimate its token count
pub trait TokensEstimatable {
    type Estimator: Tokenizer;

    fn estimate_tokens(&self, estimator: Self::Estimator) -> usize;
    //     estimator.estimate_str(self.as_ref())
    // };
}

pub trait Tokenizer {
    fn estimate_str<VAL: AsRef<str>>(&self, text: VAL) -> usize;
}
//
// impl<V: AsRef<str>> TokensEstimatable for V {
//     type Estimator = Box<dyn Tokenizer>;
//
//     fn estimate_tokens(&self) -> usize {
//         Self::Estimator.estimate_str(self.as_ref())
//     }
// }
