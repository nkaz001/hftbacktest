extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::{
    braced,
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    Error,
    Token,
};

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
                    let at_ident = &asset_type.name;
                    let at_args = &asset_type.args;

                    let lm_ident = &latency_model.name;
                    let lm_args = &latency_model.args;

                    let qm_ident = &queue_model.name;
                    let qm_args = &queue_model.args;

                    let em_ident = &exchange_model.name;
                    let em_args = &exchange_model.args;

                    let prob_func_ident = Ident::new(&format!("{}Func", qm_ident), qm_ident.span());

                    let qm_construct = if qm_ident.to_string().contains("ProbQueueModel") {
                        quote! {
                            ProbQueueModel::<#prob_func_ident, #marketdepth>::new(#prob_func_ident::new(#(#qm_args.clone()),*));
                        }
                    } else {
                        quote! {
                            #qm_ident::new();
                        }
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
                        ) => {
                            let cache = Cache::new();
                            let mut reader = Reader::new(cache);

                            for data in #asset.data.iter() {
                                match data {
                                    DataSource::File(file) => {
                                        reader.add_file(file.to_string());
                                    }
                                    DataSource::Data(data) => {
                                        reader.add_data(data.clone());
                                    }
                                }
                            }

                            let ob_local_to_exch = OrderBus::new();
                            let ob_exch_to_local = OrderBus::new();

                            let asset_type = #at_ident::new(#(#at_args.clone()),*);
                            let latency_model = #lm_ident::new(#(#lm_args.clone()),*);

                            let mut market_depth = #depth_construct;
                            match #asset.initial_snapshot.as_ref() {
                                Some(DataSource::File(file)) => {
                                    let data = read_npz(&file).unwrap();
                                    market_depth.apply_snapshot(&data);
                                }
                                Some(DataSource::Data(data)) => {
                                    market_depth.apply_snapshot(data);
                                }
                                None => {}
                            }

                            let local: Box<dyn LocalProcessor<#marketdepth, Event>> = Box::new(Local::new(
                                reader.clone(),
                                market_depth,
                                State::new(asset_type.clone(), #asset.maker_fee, #asset.taker_fee),
                                latency_model.clone(),
                                #asset.trade_len,
                                ob_local_to_exch.clone(),
                                ob_exch_to_local.clone(),
                            ));

                            let mut market_depth = #depth_construct;
                            match #asset.initial_snapshot.as_ref() {
                                Some(DataSource::File(file)) => {
                                    let data = read_npz(&file).unwrap();
                                    market_depth.apply_snapshot(&data);
                                }
                                Some(DataSource::Data(data)) => {
                                    market_depth.apply_snapshot(data);
                                }
                                None => {}
                            }

                            let queue_model = #qm_construct;

                            let exch: Box<dyn Processor> = Box::new(#em_ident::new(
                                reader,
                                market_depth,
                                State::new(asset_type, #asset.maker_fee, #asset.taker_fee),
                                latency_model,
                                queue_model,
                                ob_exch_to_local,
                                ob_local_to_exch,
                            ));

                            Asset {
                                local,
                                exch
                            }
                        },
                    });
                }
            }
        }
    }

    let output = quote! {
        match (
            &#asset.asset_type,
            &#asset.latency_model,
            &#asset.queue_model,
            &#asset.exch_kind
        ) {
            #(#arms)*
        }
    };

    output.into()
}
