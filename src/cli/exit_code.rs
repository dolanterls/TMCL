use crate::core::error::ErrorCategory;

pub fn for_category(category: ErrorCategory) -> u8 {
    match category {
        ErrorCategory::Config => 2,
        ErrorCategory::Network | ErrorCategory::Download => 3,
        ErrorCategory::Version | ErrorCategory::Java => 4,
        ErrorCategory::Account => 5,
        ErrorCategory::Loader => 6,
        ErrorCategory::Launch => 7,
        ErrorCategory::Terminal => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::for_category;
    use crate::core::error::ErrorCategory;

    #[test]
    fn keeps_exit_codes_stable_by_category() {
        assert_eq!(for_category(ErrorCategory::Config), 2);
        assert_eq!(for_category(ErrorCategory::Network), 3);
        assert_eq!(for_category(ErrorCategory::Java), 4);
        assert_eq!(for_category(ErrorCategory::Account), 5);
        assert_eq!(for_category(ErrorCategory::Loader), 6);
        assert_eq!(for_category(ErrorCategory::Launch), 7);
        assert_eq!(for_category(ErrorCategory::Terminal), 8);
    }
}
