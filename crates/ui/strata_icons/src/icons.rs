//! Typed Icon Constructors and Dynamic Icon Component for Strata.

#![allow(non_snake_case)]

use strata_dom::View;
use strata_macro::{component, rsx};
use crate::registry::{lookup_icon, IconName};

/// Generic dynamic vector icon component.
#[component]
pub fn Icon(
    name: IconName,
    class: Option<&'static str>,
) -> View {
    let data = lookup_icon(name);
    let user_class = class.unwrap_or("w-4 h-4");
    let combined_class = format!("inline-block flex-shrink-0 select-none stroke-current fill-none {user_class}");

    rsx! {
        <svg
            class={combined_class}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
        >
            <path
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                d={data.svg_path}
            />
        </svg>
    }
}

/// Advanced configurable icon component with size and stroke width overrides.
#[component]
pub fn IconConfig(
    name: IconName,
    size: Option<f32>,
    stroke_width: Option<f32>,
    class: Option<&'static str>,
) -> View {
    let data = lookup_icon(name);
    let user_class = class.unwrap_or("");
    let combined_class = format!("inline-block flex-shrink-0 select-none stroke-current fill-none {user_class}");
    let sw = stroke_width.unwrap_or(2.0).to_string();

    let style = if let Some(sz) = size {
        format!("width: {sz}px; height: {sz}px;")
    } else {
        String::new()
    };

    rsx! {
        <svg
            class={combined_class}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            style={style}
        >
            <path
                stroke-width={sw}
                stroke-linecap="round"
                stroke-linejoin="round"
                d={data.svg_path}
            />
        </svg>
    }
}

/// Typed static icon convenience components.
pub struct Icons;

impl Icons {
    // Navigation
    pub fn ArrowLeft(class: Option<&'static str>) -> View { Icon(IconName::ArrowLeft, class) }
    pub fn ArrowRight(class: Option<&'static str>) -> View { Icon(IconName::ArrowRight, class) }
    pub fn ArrowUp(class: Option<&'static str>) -> View { Icon(IconName::ArrowUp, class) }
    pub fn ArrowDown(class: Option<&'static str>) -> View { Icon(IconName::ArrowDown, class) }
    pub fn ArrowUpRight(class: Option<&'static str>) -> View { Icon(IconName::ArrowUpRight, class) }
    pub fn ArrowDownLeft(class: Option<&'static str>) -> View { Icon(IconName::ArrowDownLeft, class) }
    pub fn ChevronLeft(class: Option<&'static str>) -> View { Icon(IconName::ChevronLeft, class) }
    pub fn ChevronRight(class: Option<&'static str>) -> View { Icon(IconName::ChevronRight, class) }
    pub fn ChevronUp(class: Option<&'static str>) -> View { Icon(IconName::ChevronUp, class) }
    pub fn ChevronDown(class: Option<&'static str>) -> View { Icon(IconName::ChevronDown, class) }
    pub fn ChevronsUpDown(class: Option<&'static str>) -> View { Icon(IconName::ChevronsUpDown, class) }
    pub fn ChevronsLeft(class: Option<&'static str>) -> View { Icon(IconName::ChevronsLeft, class) }
    pub fn ChevronsRight(class: Option<&'static str>) -> View { Icon(IconName::ChevronsRight, class) }
    pub fn Menu(class: Option<&'static str>) -> View { Icon(IconName::Menu, class) }
    pub fn ExternalLink(class: Option<&'static str>) -> View { Icon(IconName::ExternalLink, class) }
    pub fn Compass(class: Option<&'static str>) -> View { Icon(IconName::Compass, class) }
    pub fn CornerDownRight(class: Option<&'static str>) -> View { Icon(IconName::CornerDownRight, class) }

    // Actions & Interface
    pub fn Search(class: Option<&'static str>) -> View { Icon(IconName::Search, class) }
    pub fn X(class: Option<&'static str>) -> View { Icon(IconName::X, class) }
    pub fn Plus(class: Option<&'static str>) -> View { Icon(IconName::Plus, class) }
    pub fn Minus(class: Option<&'static str>) -> View { Icon(IconName::Minus, class) }
    pub fn Check(class: Option<&'static str>) -> View { Icon(IconName::Check, class) }
    pub fn CheckCheck(class: Option<&'static str>) -> View { Icon(IconName::CheckCheck, class) }
    pub fn Copy(class: Option<&'static str>) -> View { Icon(IconName::Copy, class) }
    pub fn Edit(class: Option<&'static str>) -> View { Icon(IconName::Edit, class) }
    pub fn Trash(class: Option<&'static str>) -> View { Icon(IconName::Trash, class) }
    pub fn Download(class: Option<&'static str>) -> View { Icon(IconName::Download, class) }
    pub fn Upload(class: Option<&'static str>) -> View { Icon(IconName::Upload, class) }
    pub fn Share(class: Option<&'static str>) -> View { Icon(IconName::Share, class) }
    pub fn Filter(class: Option<&'static str>) -> View { Icon(IconName::Filter, class) }
    pub fn Sliders(class: Option<&'static str>) -> View { Icon(IconName::Sliders, class) }
    pub fn Eye(class: Option<&'static str>) -> View { Icon(IconName::Eye, class) }
    pub fn EyeOff(class: Option<&'static str>) -> View { Icon(IconName::EyeOff, class) }
    pub fn Lock(class: Option<&'static str>) -> View { Icon(IconName::Lock, class) }
    pub fn Unlock(class: Option<&'static str>) -> View { Icon(IconName::Unlock, class) }
    pub fn Settings(class: Option<&'static str>) -> View { Icon(IconName::Settings, class) }
    pub fn RefreshCw(class: Option<&'static str>) -> View { Icon(IconName::RefreshCw, class) }
    pub fn RotateCw(class: Option<&'static str>) -> View { Icon(IconName::RotateCw, class) }
    pub fn MoreHorizontal(class: Option<&'static str>) -> View { Icon(IconName::MoreHorizontal, class) }
    pub fn MoreVertical(class: Option<&'static str>) -> View { Icon(IconName::MoreVertical, class) }
    pub fn Maximize(class: Option<&'static str>) -> View { Icon(IconName::Maximize, class) }
    pub fn Minimize(class: Option<&'static str>) -> View { Icon(IconName::Minimize, class) }

    // Dev & Infrastructure
    pub fn Server(class: Option<&'static str>) -> View { Icon(IconName::Server, class) }
    pub fn Cpu(class: Option<&'static str>) -> View { Icon(IconName::Cpu, class) }
    pub fn HardDrive(class: Option<&'static str>) -> View { Icon(IconName::HardDrive, class) }
    pub fn Database(class: Option<&'static str>) -> View { Icon(IconName::Database, class) }
    pub fn Network(class: Option<&'static str>) -> View { Icon(IconName::Network, class) }
    pub fn Activity(class: Option<&'static str>) -> View { Icon(IconName::Activity, class) }
    pub fn Terminal(class: Option<&'static str>) -> View { Icon(IconName::Terminal, class) }
    pub fn Code(class: Option<&'static str>) -> View { Icon(IconName::Code, class) }
    pub fn GitBranch(class: Option<&'static str>) -> View { Icon(IconName::GitBranch, class) }
    pub fn GitCommit(class: Option<&'static str>) -> View { Icon(IconName::GitCommit, class) }
    pub fn GitPullRequest(class: Option<&'static str>) -> View { Icon(IconName::GitPullRequest, class) }
    pub fn Shield(class: Option<&'static str>) -> View { Icon(IconName::Shield, class) }
    pub fn ShieldCheck(class: Option<&'static str>) -> View { Icon(IconName::ShieldCheck, class) }
    pub fn Zap(class: Option<&'static str>) -> View { Icon(IconName::Zap, class) }
    pub fn Cloud(class: Option<&'static str>) -> View { Icon(IconName::Cloud, class) }
    pub fn Layers(class: Option<&'static str>) -> View { Icon(IconName::Layers, class) }
    pub fn Box(class: Option<&'static str>) -> View { Icon(IconName::Box, class) }

    // Media & Audio
    pub fn Play(class: Option<&'static str>) -> View { Icon(IconName::Play, class) }
    pub fn Pause(class: Option<&'static str>) -> View { Icon(IconName::Pause, class) }
    pub fn Volume(class: Option<&'static str>) -> View { Icon(IconName::Volume, class) }
    pub fn Volume1(class: Option<&'static str>) -> View { Icon(IconName::Volume1, class) }
    pub fn Volume2(class: Option<&'static str>) -> View { Icon(IconName::Volume2, class) }
    pub fn VolumeX(class: Option<&'static str>) -> View { Icon(IconName::VolumeX, class) }
    pub fn Mic(class: Option<&'static str>) -> View { Icon(IconName::Mic, class) }
    pub fn MicOff(class: Option<&'static str>) -> View { Icon(IconName::MicOff, class) }
    pub fn Image(class: Option<&'static str>) -> View { Icon(IconName::Image, class) }
    pub fn Video(class: Option<&'static str>) -> View { Icon(IconName::Video, class) }
    pub fn Music(class: Option<&'static str>) -> View { Icon(IconName::Music, class) }
    pub fn Camera(class: Option<&'static str>) -> View { Icon(IconName::Camera, class) }
    pub fn Radio(class: Option<&'static str>) -> View { Icon(IconName::Radio, class) }
    pub fn Headphones(class: Option<&'static str>) -> View { Icon(IconName::Headphones, class) }

    // Communication & Social
    pub fn Mail(class: Option<&'static str>) -> View { Icon(IconName::Mail, class) }
    pub fn MessageSquare(class: Option<&'static str>) -> View { Icon(IconName::MessageSquare, class) }
    pub fn MessageCircle(class: Option<&'static str>) -> View { Icon(IconName::MessageCircle, class) }
    pub fn Send(class: Option<&'static str>) -> View { Icon(IconName::Send, class) }
    pub fn Bell(class: Option<&'static str>) -> View { Icon(IconName::Bell, class) }
    pub fn BellOff(class: Option<&'static str>) -> View { Icon(IconName::BellOff, class) }
    pub fn Phone(class: Option<&'static str>) -> View { Icon(IconName::Phone, class) }
    pub fn PhoneCall(class: Option<&'static str>) -> View { Icon(IconName::PhoneCall, class) }
    pub fn User(class: Option<&'static str>) -> View { Icon(IconName::User, class) }
    pub fn Users(class: Option<&'static str>) -> View { Icon(IconName::Users, class) }
    pub fn UserPlus(class: Option<&'static str>) -> View { Icon(IconName::UserPlus, class) }
    pub fn Globe(class: Option<&'static str>) -> View { Icon(IconName::Globe, class) }
    pub fn Bookmark(class: Option<&'static str>) -> View { Icon(IconName::Bookmark, class) }
    pub fn Heart(class: Option<&'static str>) -> View { Icon(IconName::Heart, class) }
    pub fn Star(class: Option<&'static str>) -> View { Icon(IconName::Star, class) }
    pub fn Sparkles(class: Option<&'static str>) -> View { Icon(IconName::Sparkles, class) }

    // Commerce & Finance
    pub fn ShoppingCart(class: Option<&'static str>) -> View { Icon(IconName::ShoppingCart, class) }
    pub fn ShoppingBag(class: Option<&'static str>) -> View { Icon(IconName::ShoppingBag, class) }
    pub fn CreditCard(class: Option<&'static str>) -> View { Icon(IconName::CreditCard, class) }
    pub fn DollarSign(class: Option<&'static str>) -> View { Icon(IconName::DollarSign, class) }
    pub fn Percent(class: Option<&'static str>) -> View { Icon(IconName::Percent, class) }
    pub fn Tag(class: Option<&'static str>) -> View { Icon(IconName::Tag, class) }
    pub fn Package(class: Option<&'static str>) -> View { Icon(IconName::Package, class) }
    pub fn Truck(class: Option<&'static str>) -> View { Icon(IconName::Truck, class) }
    pub fn Wallet(class: Option<&'static str>) -> View { Icon(IconName::Wallet, class) }

    // Status & Feedback
    pub fn CheckCircle(class: Option<&'static str>) -> View { Icon(IconName::CheckCircle, class) }
    pub fn CheckCircle2(class: Option<&'static str>) -> View { Icon(IconName::CheckCircle2, class) }
    pub fn AlertCircle(class: Option<&'static str>) -> View { Icon(IconName::AlertCircle, class) }
    pub fn AlertTriangle(class: Option<&'static str>) -> View { Icon(IconName::AlertTriangle, class) }
    pub fn AlertOctagon(class: Option<&'static str>) -> View { Icon(IconName::AlertOctagon, class) }
    pub fn Info(class: Option<&'static str>) -> View { Icon(IconName::Info, class) }
    pub fn HelpCircle(class: Option<&'static str>) -> View { Icon(IconName::HelpCircle, class) }
    pub fn Clock(class: Option<&'static str>) -> View { Icon(IconName::Clock, class) }
    pub fn Flame(class: Option<&'static str>) -> View { Icon(IconName::Flame, class) }
    pub fn Loader(class: Option<&'static str>) -> View { Icon(IconName::Loader, class) }
    pub fn Loader2(class: Option<&'static str>) -> View { Icon(IconName::Loader2, class) }
    pub fn Sun(class: Option<&'static str>) -> View { Icon(IconName::Sun, class) }
    pub fn Moon(class: Option<&'static str>) -> View { Icon(IconName::Moon, class) }

    // Devices & Hardware
    pub fn Smartphone(class: Option<&'static str>) -> View { Icon(IconName::Smartphone, class) }
    pub fn Tablet(class: Option<&'static str>) -> View { Icon(IconName::Tablet, class) }
    pub fn Laptop(class: Option<&'static str>) -> View { Icon(IconName::Laptop, class) }
    pub fn Monitor(class: Option<&'static str>) -> View { Icon(IconName::Monitor, class) }
    pub fn Wifi(class: Option<&'static str>) -> View { Icon(IconName::Wifi, class) }
    pub fn WifiOff(class: Option<&'static str>) -> View { Icon(IconName::WifiOff, class) }
    pub fn Bluetooth(class: Option<&'static str>) -> View { Icon(IconName::Bluetooth, class) }
    pub fn FoldHorizontal(class: Option<&'static str>) -> View { Icon(IconName::FoldHorizontal, class) }
    pub fn FoldVertical(class: Option<&'static str>) -> View { Icon(IconName::FoldVertical, class) }
}
