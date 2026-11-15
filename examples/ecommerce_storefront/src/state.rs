//! State model for Apex Athletics E-Commerce Storefront.

use strata::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub price: f64,
    pub original_price: Option<f64>,
    pub rating: f32,
    pub review_count: usize,
    pub colors: Vec<&'static str>,
    pub badge: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CartItem {
    pub product: Product,
    pub quantity: usize,
    pub selected_color: &'static str,
}

#[derive(Clone)]
pub struct StoreState {
    pub products: Signal<Vec<Product>>,
    pub cart: Signal<Vec<CartItem>>,
    pub is_cart_open: Signal<bool>,
    pub selected_category: Signal<String>,
    pub search_query: Signal<String>,
    pub promo_code: Signal<String>,
    pub discount_percent: Signal<f32>,
}

impl StoreState {
    pub fn new() -> Self {
        let catalog = vec![
            Product {
                id: "prod-01",
                name: "Apex Aerotech Seamless Compression Top",
                category: "Apparel",
                price: 78.00,
                original_price: Some(95.00),
                rating: 4.9,
                review_count: 248,
                colors: vec!["#0a0a0a", "#4338ca", "#065f46"],
                badge: Some("BESTSELLER"),
            },
            Product {
                id: "prod-02",
                name: "Kevlar Hybrid Carbon Running Shorts 5\"",
                category: "Apparel",
                price: 64.00,
                original_price: None,
                rating: 4.8,
                review_count: 182,
                colors: vec!["#171717", "#52525b"],
                badge: None,
            },
            Product {
                id: "prod-03",
                name: "Strata Pace Pro Carbon Plate Racers",
                category: "Footwear",
                price: 240.00,
                original_price: Some(280.00),
                rating: 5.0,
                review_count: 512,
                colors: vec!["#fafafa", "#f97316", "#06b6d4"],
                badge: Some("NEW RELEASE"),
            },
            Product {
                id: "prod-04",
                name: "Ultralight Cordura Waterproof Trail Vest",
                category: "Gear",
                price: 135.00,
                original_price: None,
                rating: 4.7,
                review_count: 94,
                colors: vec!["#262626", "#d97706"],
                badge: None,
            },
            Product {
                id: "prod-05",
                name: "Merino Wool Thermal Performance Base Layer",
                category: "Apparel",
                price: 92.00,
                original_price: None,
                rating: 4.9,
                review_count: 310,
                colors: vec!["#18181b", "#3f3f46"],
                badge: None,
            },
            Product {
                id: "prod-06",
                name: "Graphene Electrolyte Hydration Flask 750ml",
                category: "Gear",
                price: 45.00,
                original_price: Some(55.00),
                rating: 4.6,
                review_count: 140,
                colors: vec!["#09090b", "#475569"],
                badge: Some("POPULAR"),
            },
        ];

        Self {
            products: use_signal(catalog),
            cart: use_signal(Vec::new()),
            is_cart_open: use_signal(false),
            selected_category: use_signal(String::from("ALL")),
            search_query: use_signal(String::new()),
            promo_code: use_signal(String::new()),
            discount_percent: use_signal(0.0),
        }
    }

    pub fn add_to_cart(&self, product: Product, color: &'static str) {
        self.cart.update(|items| {
            if let Some(existing) = items
                .iter_mut()
                .find(|i| i.product.id == product.id && i.selected_color == color)
            {
                existing.quantity += 1;
            } else {
                items.push(CartItem {
                    product,
                    quantity: 1,
                    selected_color: color,
                });
            }
        });
        self.is_cart_open.set(true);
    }

    pub fn cart_count(&self) -> usize {
        self.cart.get().iter().map(|item| item.quantity).sum()
    }

    pub fn subtotal(&self) -> f64 {
        self.cart
            .get()
            .iter()
            .map(|item| item.product.price * item.quantity as f64)
            .sum()
    }
}
