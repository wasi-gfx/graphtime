use clap::Parser;
use frame_buffer_wasmtime::{FrameBufferCtx, FrameBufferCtxView};
use std::sync::Arc;
use surface_wasmtime::{
    winit::WasiWinitEventLoopProxy, SurfaceCtx, SurfaceCtxView, SurfaceFrameBufferCtx,
    SurfaceFrameBufferCtxView, SurfaceWebgpuCtx, SurfaceWebgpuCtxView,
};
use wasi_webgpu_wasmtime::{
    reexports::{wgpu_core, wgpu_types},
    WasiWebGpuCtx, WasiWebGpuCtxView, WasiWebGpuOptions,
};
use wasmtime::{
    component::{Component, Linker},
    error::Context,
    Config, Engine, Store,
};

use wasmtime_wasi::{
    p3::bindings::Command, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    file: String,
}

struct HostState {
    table: ResourceTable,
    ctx: WasiCtx,
    instance: Arc<wgpu_core::global::Global>,
    webgpu_options: WasiWebGpuOptions,
    main_thread_proxy: WasiWinitEventLoopProxy,
}

impl HostState {
    fn new(main_thread_proxy: WasiWinitEventLoopProxy) -> Self {
        Self {
            table: ResourceTable::new(),
            ctx: WasiCtxBuilder::new().inherit_stdio().build(),
            instance: Arc::new(wgpu_core::global::Global::new(
                "webgpu",
                wgpu_types::InstanceDescriptor {
                    backends: wgpu_types::Backends::all(),
                    flags: wgpu_types::InstanceFlags::from_build_config(),
                    backend_options: wgpu_types::BackendOptions::default(),
                    memory_budget_thresholds: wgpu_types::MemoryBudgetThresholds::default(),
                    display: None,
                },
                None,
            )),
            webgpu_options: WasiWebGpuOptions::default(),
            main_thread_proxy,
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl WasiWebGpuCtxView for HostState {
    fn webgpu_ctx(&mut self) -> WasiWebGpuCtx<'_> {
        WasiWebGpuCtx {
            instance: &self.instance,
            table: &mut self.table,
            options: &self.webgpu_options,
        }
    }
}

impl FrameBufferCtxView for HostState {
    fn frame_buffer_ctx<'a>(&'a mut self) -> FrameBufferCtx<'a> {
        FrameBufferCtx {
            table: &mut self.table,
        }
    }
}

impl SurfaceCtxView for HostState {
    type Spawner = WasiWinitEventLoopProxy;
    fn surface_ctx(&mut self) -> SurfaceCtx<'_, WasiWinitEventLoopProxy> {
        SurfaceCtx {
            table: &mut self.table,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

impl SurfaceWebgpuCtxView for HostState {
    type Spawner = WasiWinitEventLoopProxy;
    fn surface_webgpu_ctx(&mut self) -> SurfaceWebgpuCtx<'_, WasiWinitEventLoopProxy> {
        SurfaceWebgpuCtx {
            table: &mut self.table,
            instance: &self.instance,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

impl SurfaceFrameBufferCtxView for HostState {
    type Spawner = WasiWinitEventLoopProxy;
    fn surface_frame_buffer_ctx(&mut self) -> SurfaceFrameBufferCtx<'_, WasiWinitEventLoopProxy> {
        SurfaceFrameBufferCtx {
            table: &mut self.table,
            instance: &self.instance,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

#[tokio::main]
async fn main() -> wasmtime::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Args::parse();

    let mut config = Config::default();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let mut linker: Linker<HostState> = Linker::new(&engine);

    wasi_webgpu_wasmtime::add_to_linker(&mut linker)?;
    frame_buffer_wasmtime::add_to_linker(&mut linker)?;
    surface_wasmtime::add_all_to_linker(&mut linker)?;
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;

    let (main_thread_loop, main_thread_proxy) =
        surface_wasmtime::winit::create_wasi_winit_event_loop();
    let host_state = HostState::new(main_thread_proxy);

    let mut store = Store::new(&engine, host_state);

    let component =
        Component::from_file(&engine, &args.file).context("Component file not found")?;

    let command = Command::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    tokio::spawn(async move {
        store
            .run_concurrent(async move |store| command.wasi_cli_run().call_run(store).await)
            .await
            .expect("failed to run component")
            .expect("failed to invoke `run` function")
            .expect("`run` returned an error");
    });

    main_thread_loop.run();

    Ok(())
}
