/// Gets price precision.
///
/// * `tick_size` - This should not be a computed value.
pub fn get_precision(tick_size: f32) -> usize {
    let s = tick_size.to_string();
    let mut prec = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '.' {
            prec = s.len() - i - 1;
            break;
        }
    }
    prec
}
