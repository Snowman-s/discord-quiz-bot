use tracing::{error, info};

pub(crate) fn roughly_card_name_equal(
    input_card_name: &str,
    card_name: &str,
    card_name_ruby: &str,
) -> bool {
    let eq = |input: &str, target: &str| -> bool {
        info!(
            "Check `{}` is valid representation for `{}` ",
            input, target
        );

        let ichars = input.chars().collect::<Vec<_>>();
        let tchars = target.chars().collect::<Vec<_>>();

        let mut i_index = 0;

        for t in tchars {
            let i_opt = ichars.get(i_index);

            match t {
                '０'..='９' => {
                    let diff = (t as u32) - ('０' as u32);
                    let cmp = char::from_u32(('0' as u32) + diff).unwrap();
                    if let Some(i) = i_opt {
                        if i == &cmp {
                            i_index += 1;
                            continue;
                        }
                    }
                    info!("{:?} cannot applicatable for {} (=>{})", i_opt, t, cmp);
                }
                '0'..='9' => {
                    let diff = (t as u32) - ('0' as u32);
                    let cmp = char::from_u32(('０' as u32) + diff).unwrap();
                    if let Some(i) = i_opt {
                        if i == &cmp {
                            i_index += 1;
                            continue;
                        }
                    }
                    info!("{:?} cannot applicatable for {} (=>{})", i_opt, t, cmp);
                }
                'ァ'..='ン' => {
                    let cmp = char::from_u32((t as u32) - ('ァ' as u32) + ('ぁ' as u32)).unwrap();
                    if let Some(i) = i_opt {
                        if i == &cmp {
                            i_index += 1;
                            continue;
                        }
                    }
                    info!(
                        "{:?} cannot applicatable for {} (=> {} ({}))",
                        i_opt,
                        t,
                        cmp,
                        (t as u32) - ('ァ' as u32) + ('ぁ' as u32)
                    );
                }
                ' ' | '　' => {
                    if let Some(i) = i_opt {
                        if i == &' ' || i == &'　' {
                            i_index += 1;
                        }
                    }
                    continue;
                }
                '―' | '－' => {
                    if let Some(i) = i_opt {
                        if i == &'―' || i == &'－' || i == &'ー' {
                            i_index += 1;
                        }
                    }
                    continue;
                }
                '・' | '.' | '．' => {
                    if let Some(i) = i_opt {
                        if i == &t {
                            i_index += 1;
                        }
                    }
                    continue;
                }
                _ => {}
            }
            if let Some(i) = i_opt {
                if i == &t {
                    i_index += 1;
                    continue;
                }
            }
            error!("{:?} cannot applicatable for {}", i_opt, t);
            return false;
        }

        // icharsの最後の方の空白を送る
        if ichars[i_index..].iter().any(|c| c != &' ' && c != &'　') {
            return false;
        }

        // 上と統合すべきかもしれないが、この書き方の方が「基本true、どこかでうまくいかなかったらfalse」という雰囲気になると思われる
        true
    };
    eq(input_card_name, card_name) || eq(input_card_name, card_name_ruby)
}
