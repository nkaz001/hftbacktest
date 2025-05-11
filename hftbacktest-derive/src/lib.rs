extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{
    self,
    Data,
    DeriveInput,
    Error,
    Fields,
    Token,
    braced,
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

#[proc_macro_derive(NpyDTyped)]
pub fn dtype_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;

    let mut field_names = Vec::new();
    let mut field_types = Vec::new();

    let expanded = match input.data {
        Data::Struct(ref data_struct) => {
            if let Fields::Named(ref fields_named) = data_struct.fields {
                for field in fields_named.named.iter() {
                    let field_name = field.ident.as_ref().unwrap().to_string();
                    let field_type = field.ty.clone();

                    let ty_str = quote! { #field_type }.to_string();
                    let endianess = if is_little_endian() { "<" } else { ">" };
                    let ty = match ty_str.as_str() {
                        "f64" => "f8",
                        "f32" => "f4",
                        "f16" => "f2",
                        "f8" => "f1",
                        "i64" => "i8",
                        "i32" => "i4",
                        "i16" => "i2",
                        "i8" => "i1",
                        "u64" => "u8",
                        "u32" => "u4",
                        "u16" => "u2",
                        "u8" => "u1",
                        "bool" => "bool",
                        s => panic!("\"{field_name}: {s}\": {s} is unsupported."),
                    };

                    field_names.push(field_name);
                    field_types.push(endianess.to_string() + ty);
                }
            }

            // Generate code to print field names and types
            quote! {
                impl crate::backtest::data::NpyDTyped for #name {
                    fn descr() -> Vec<crate::backtest::data::Field> {
                        return vec![
                            #(
                                crate::backtest::data::Field {
                                    name: #field_names.to_string(),
                                    ty: #field_types.to_string(),
                                }
                            ),*
                        ];
                    }
                }
            }
        }
        _ => quote! {
            compile_error!("must be a struct");
        },
    };

    expanded.into()
}

fn is_little_endian() -> bool {
    let n: u32 = 1;
    if n.to_be() == n {
        false
    } else if n.to_le() == n {
        true
    } else {
        panic!();
    }
}

struct EnumArgs {
    name: Ident,
    args: Vec<Ident>,
}

impl Parse for EnumArgs {
    fn parse(input: ParseStream) -> Result<Self, Error> {
        let mut ret = EnumArgs {
            name: input.parse()?,
            args: vec![],
        };
        let content;
        let _brace_token = braced!(content in input);
        ret.args = content
            .parse_terminated(Ident::parse, Token![,])?
            .into_iter()
            .collect();
        Ok(ret)
    }
}

struct BuildAssetInput {
    value: Ident,
    marketdepth: Ident,
    asset_type: Vec<EnumArgs>,
    latency_model: Vec<EnumArgs>,
    queue_model: Vec<EnumArgs>,
    exchange_model: Vec<EnumArgs>,
    fee_model: Vec<EnumArgs>,
}

impl Parse for BuildAssetInput {
    fn parse(input: ParseStream) -> Result<Self, Error> {
        let value = input.parse()?;
        input.parse::<syn::token::Comma>()?;
        let marketdepth = input.parse()?;

        let mut parsed_input = BuildAssetInput {
            value,
            marketdepth,
            asset_type: Default::default(),
            latency_model: Default::default(),
            queue_model: Default::default(),
            exchange_model: Default::default(),
            fee_model: Default::default(),
        };

        let mut content;

        input.parse::<syn::token::Comma>()?;
        let _bracket_token = bracketed!(content in input);
        parsed_input.asset_type = content
            .parse_terminated(EnumArgs::parse, Token![,])?
            .into_iter()
            .collect();

        input.parse::<syn::token::Comma>()?;
        let _bracket_token = bracketed!(content in input);
        parsed_input.latency_model = content
            .parse_terminated(EnumArgs::parse, Token![,])?
            .into_iter()
            .collect();

        input.parse::<syn::token::Comma>()?;
        let _bracket_token = bracketed!(content in input);
        parsed_input.queue_model = content
            .parse_terminated(EnumArgs::parse, Token![,])?
            .into_iter()
            .collect();

        input.parse::<syn::token::Comma>()?;
        let _bracket_token = bracketed!(content in input);
        parsed_input.exchange_model = content
            .parse_terminated(EnumArgs::parse, Token![,])?
            .into_iter()
            .collect();

        input.parse::<syn::token::Comma>()?;
        let _bracket_token = bracketed!(content in input);
        parsed_input.fee_model = content
            .parse_terminated(EnumArgs::parse, Token![,])?
            .into_iter()
            .collect();

        Ok(parsed_input)
    }
}

#[proc_macro]
pub fn build_asset(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuildAssetInput);
    let asset = input.value;
    let marketdepth = input.marketdepth;

    // Generates match arms for all combinations.
    let mut arms = Vec::new();
    for asset_type in input.asset_type.iter() {
        for latency_model in input.latency_model.iter() {
            for queue_model in input.queue_model.iter() {
                for exchange_model in input.exchange_model.iter() {
                    for fee_model in input.fee_model.iter() {
                        let at_ident = &asset_type.name;
                        let at_args = &asset_type.args;

                        let lm_ident = &latency_model.name;
                        let lm_args = &latency_model.args;

                        let qm_ident = &queue_model.name;
                        let qm_args = &queue_model.args;

                        let em_ident = &exchange_model.name;
                        let em_args = &exchange_model.args;

                        let fm_ident = &fee_model.name;
                        let fm_args = &fee_model.args;

                        let prob_func_ident =
                            Ident::new(&format!("{}Func", qm_ident), Span::call_site());

                        let qm_ident_str = qm_ident.to_string();
                        let qm_construct = if qm_ident_str.contains("ProbQueueModel") {
                            quote! {
                                ProbQueueModel::<#prob_func_ident, #marketdepth>::new(#prob_func_ident::new(#(#qm_args.clone()),*));
                            }
                        } else {
                            quote! {
                                #qm_ident::new();
                            }
                        };

                        let l3 = qm_ident_str == "L3FIFOQueueModel";
                        let (local_ident, exch_ident) = if l3 {
                            // todo: L3PartialFillExchange is unsupported. This is verified within
                            //  the build functions in the py-hftbacktest module.
                            (
                                Ident::new("L3Local", Span::call_site()),
                                Ident::new("L3NoPartialFillExchange", Span::call_site()),
                            )
                        } else {
                            (Ident::new("Local", Span::call_site()), em_ident.clone())
                        };

                        let depth_construct = match marketdepth.to_string().as_str() {
                            "HashMapMarketDepth" => {
                                quote! {
                                    #marketdepth::new(#asset.tick_size, #asset.lot_size);
                                }
                            }
                            "ROIVectorMarketDepth" => {
                                quote! {
                                    #marketdepth::new(
                                        #asset.tick_size,
                                        #asset.lot_size,
                                        #asset.roi_lb,
                                        #asset.roi_ub
                                    );
                                }
                            }
                            _ => panic!(),
                        };

                        arms.push(quote! {
                        (
                            AssetType::#at_ident { #(#at_args),* },
                            LatencyModel::#lm_ident { #(#lm_args),* },
                            QueueModel::#qm_ident { #(#qm_args),* },
                            ExchangeKind::#em_ident { #(#em_args),* },
                            FeeModel::#fm_ident { #(#fm_args),* },
                        ) => {
                            let reader = if #asset.latency_offset == 0 {
                                Reader::builder()
                                    .parallel_load(#asset.parallel_load)
                                    .data(#asset.data.clone())
                                    .build()
                                    .unwrap()
                            } else {
                                Reader::builder()
                                    .parallel_load(#asset.parallel_load)
                                    .data(#asset.data.clone())
                                    .preprocessor(FeedLatencyAdjustment::new(#asset.latency_offset))
                                    .build()
                                    .unwrap()
                            };

                            let asset_type = #at_ident::new(#(#at_args.clone()),*);
                            let latency_model = #lm_ident::new(#(#lm_args.clone()),*);
                            let fee_model = #fm_ident::new(#(#fm_args.clone()),*);

                            let (order_e2l, order_l2e) = order_bus(latency_model);

                            let mut market_depth = #depth_construct;
                            match #asset.initial_snapshot.as_ref() {
                                Some(DataSource::File(file)) => {
                                    let data = read_npz_file(&file, "data").unwrap();
                                    market_depth.apply_snapshot(&data);
                                }
                                Some(DataSource::Data(data)) => {
                                    market_depth.apply_snapshot(data);
                                }
                                None => {}
                            }

                            let local: Box<dyn LocalProcessor<#marketdepth>> = Box::new(#local_ident::new(
                                market_depth,
                                State::new(asset_type.clone(), fee_model.clone()),
                                #asset.last_trades_cap,
                                order_l2e,
                            ));

                            let mut market_depth = #depth_construct;
                            match #asset.initial_snapshot.as_ref() {
                                Some(DataSource::File(file)) => {
                                    let data = read_npz_file(&file, "data").unwrap();
                                    market_depth.apply_snapshot(&data);
                                }
                                Some(DataSource::Data(data)) => {
                                    market_depth.apply_snapshot(data);
                                }
                                None => {}
                            }

                            let queue_model = #qm_construct;

                            let exch: Box<dyn Processor> = Box::new(#exch_ident::new(
                                market_depth,
                                State::new(asset_type, fee_model.clone()),
                                queue_model,
                                order_e2l,
                            ));

                            Asset {
                                local,
                                exch,
                                reader
                            }
                        },
                    });
                    }
                }
            }
        }
    }

    let output = quote! {
        match (
            &#asset.asset_type,
            &#asset.latency_model,
            &#asset.queue_model,
            &#asset.exch_kind,
            &#asset.fee_model,
        ) {
            #(#arms)*
        }
    };

    output.into()
}
