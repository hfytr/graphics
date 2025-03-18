use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    util::read_spv,
    vk, Device, Entry, Instance,
};
use std::io::Cursor;
use std::{ffi::CStr, os::raw::c_char};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowAttributes},
};

const MAX_IN_FLIGHT: usize = 2;

struct App {
    entry: Entry,
    instance: Instance,
    window: Window,
    pdevice: vk::PhysicalDevice,
    device: Device,
    queue: vk::Queue,
    surface: vk::SurfaceKHR,
    surface_loader: surface::Instance,
    swapchain: vk::SwapchainKHR,
    swap_device: swapchain::Device,
    swap_imgs: Vec<vk::Image>,
    swap_img_views: Vec<vk::ImageView>,
    swap_framebuffers: Vec<vk::Framebuffer>,
    format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    command_pool: vk::CommandPool,
    command_buffers: [vk::CommandBuffer; MAX_IN_FLIGHT],
    image_available: [vk::Semaphore; MAX_IN_FLIGHT],
    render_done: [vk::Semaphore; MAX_IN_FLIGHT],
    in_flight: [vk::Fence; MAX_IN_FLIGHT],
    cur_frame: usize,
}
pub struct WrappedApp(Option<App>);
impl WrappedApp {
    pub fn new() -> Self {
        WrappedApp(None)
    }
}

impl App {
    fn render(&mut self) {
        let img_idx;
        unsafe {
            self.device
                .wait_for_fences(&[self.in_flight[self.cur_frame]], true, u64::MAX)
                .expect("Failed to wait for fences.");
            self.device
                .reset_fences(&[self.in_flight[self.cur_frame]])
                .expect("Failed to reset fences.");
            match self.swap_device.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available[self.cur_frame],
                vk::Fence::null(),
            ) {
                Ok((idx, _)) => img_idx = idx,
                Err(e) => {
                    if e == vk::Result::ERROR_OUT_OF_DATE_KHR || e == vk::Result::SUBOPTIMAL_KHR {
                        println!("Recreating swap chain.");
                        return;
                    } else {
                        panic!("Failed to acquire next image {}", e);
                    }
                }
            }
            self.device
                .begin_command_buffer(
                    self.command_buffers[self.cur_frame],
                    &vk::CommandBufferBeginInfo::default(),
                )
                .expect("Failed to begin command buffer.")
        }

        let mut clear_color = [vk::ClearValue::default()];
        clear_color[0].color.float32 = [0.0, 0.0, 0.0, 1.0];
        let pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.swap_framebuffers[img_idx as usize])
            .render_area(vk::Rect2D::default().extent(self.extent))
            .clear_values(&clear_color);
        unsafe {
            self.device
                .reset_command_buffer(
                    self.command_buffers[self.cur_frame],
                    vk::CommandBufferResetFlags::empty(),
                )
                .expect("Failed to reset command buffer.");
            self.device
                .begin_command_buffer(
                    self.command_buffers[self.cur_frame],
                    &vk::CommandBufferBeginInfo::default(),
                )
                .expect("Failed to begin command buffer.");
            self.device.cmd_begin_render_pass(
                self.command_buffers[self.cur_frame],
                &pass_info,
                vk::SubpassContents::INLINE,
            );
            self.device.cmd_bind_pipeline(
                self.command_buffers[self.cur_frame],
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
        }
        let viewport = [vk::Viewport::default()
            .width(self.extent.width as f32)
            .height(self.extent.height as f32)
            .max_depth(1.0)];
        let scissor = [vk::Rect2D::default().extent(self.extent)];
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffers[self.cur_frame], 0, &viewport);
            self.device
                .cmd_set_scissor(self.command_buffers[self.cur_frame], 0, &scissor);
            self.device
                .cmd_draw(self.command_buffers[self.cur_frame], 3, 1, 0, 0);
            self.device
                .cmd_end_render_pass(self.command_buffers[self.cur_frame]);
            self.device
                .end_command_buffer(self.command_buffers[self.cur_frame])
                .expect("Failed to end command buffer.");
        }

        let image_available = [self.image_available[self.cur_frame]];
        let render_done = [self.render_done[self.cur_frame]];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&image_available)
            .signal_semaphores(&render_done)
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(&self.command_buffers[self.cur_frame..=self.cur_frame]);
        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], self.in_flight[self.cur_frame])
                .expect("Faild to submit frame.")
        }
        let swapchains = [self.swapchain];
        let img_idxs = [img_idx];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&render_done)
            .swapchains(&swapchains)
            .image_indices(&img_idxs);
        unsafe {
            self.swap_device
                .queue_present(self.queue, &present_info)
                .expect("Failed to present.");
        }
        self.cur_frame = (self.cur_frame + 1) % MAX_IN_FLIGHT;
    }

    fn create_swapchain(&mut self) {
        let (capabilities, formats, modes) =
            App::get_swap_support(self.pdevice, &self.surface_loader, self.surface);
        self.format = *formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(&formats[0]);
        self.extent = if capabilities.current_extent.width == u32::MAX {
            capabilities.max_image_extent
        } else {
            capabilities.current_extent
        };
        self.swapchain = unsafe {
            let mode = *modes
                .iter()
                .find(|m| **m == vk::PresentModeKHR::MAILBOX)
                .unwrap_or(&vk::PresentModeKHR::FIFO);
            let img_count = if capabilities.max_image_count == 0
                || capabilities.max_image_count > capabilities.min_image_count
            {
                capabilities.min_image_count + 1
            } else {
                capabilities.max_image_count
            };
            let info = vk::SwapchainCreateInfoKHR::default()
                .image_format(self.format.format)
                .present_mode(mode)
                .min_image_count(img_count)
                .image_extent(self.extent)
                .image_color_space(self.format.color_space)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .queue_family_indices(&[])
                .pre_transform(capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .clipped(true)
                .surface(self.surface)
                .old_swapchain(vk::SwapchainKHR::null());
            self.swap_device
                .create_swapchain(&info, None)
                .expect("Failed to create swapchain.")
        };
        self.swap_imgs = unsafe {
            self.swap_device
                .get_swapchain_images(self.swapchain)
                .expect("Failed to retrieve swapchain image handles.")
        };
        self.swap_img_views = self
            .swap_imgs
            .iter()
            .map(|i| {
                let info = vk::ImageViewCreateInfo::default()
                    .image(*i)
                    .format(self.format.format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .components(
                        vk::ComponentMapping::default()
                            .r(vk::ComponentSwizzle::IDENTITY)
                            .g(vk::ComponentSwizzle::IDENTITY)
                            .b(vk::ComponentSwizzle::IDENTITY)
                            .a(vk::ComponentSwizzle::IDENTITY),
                    )
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                unsafe {
                    self.device
                        .create_image_view(&info, None)
                        .expect("Failed to create image view.")
                }
            })
            .collect();
        self.render_pass = unsafe {
            let attachment_desc = [vk::AttachmentDescription::default()
                .format(self.format.format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
            let attachment_ref = [vk::AttachmentReference::default()
                .attachment(0)
                // TODO: use better image layout for attachment ref
                .layout(vk::ImageLayout::GENERAL)];
            let subpass = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&attachment_ref)];
            let dependancies = [vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
            let info = vk::RenderPassCreateInfo::default()
                .attachments(&attachment_desc)
                .subpasses(&subpass)
                .dependencies(&dependancies);
            self.device
                .create_render_pass(&info, None)
                .expect("Failed to create render pass.")
        };
        self.swap_framebuffers = (0..self.swap_img_views.len())
            .map(|i| unsafe {
                let attachments = [self.swap_img_views[i]];
                let framebuffer_info = vk::FramebufferCreateInfo::default()
                    .render_pass(self.render_pass)
                    .attachments(&attachments)
                    .width(self.extent.width)
                    .height(self.extent.height)
                    .layers(1);
                self.device
                    .create_framebuffer(&framebuffer_info, None)
                    .expect("Failed to create swapchain framebuffer.")
            })
            .collect();
    }

    fn clean_swapchain(&mut self) {
        unsafe {
            for i in 0..self.swap_framebuffers.len() {
                self.device
                    .destroy_framebuffer(self.swap_framebuffers[i], None);
            }
            for i in 0..self.swap_img_views.len() {
                self.device.destroy_image_view(self.swap_img_views[i], None);
            }
            self.swap_device.destroy_swapchain(self.swapchain, None);
        }
    }

    fn get_swap_support(
        pdevice: vk::PhysicalDevice,
        surface_loader: &surface::Instance,
        surface: vk::SurfaceKHR,
    ) -> (
        vk::SurfaceCapabilitiesKHR,
        Vec<vk::SurfaceFormatKHR>,
        Vec<vk::PresentModeKHR>,
    ) {
        unsafe {
            (
                surface_loader
                    .get_physical_device_surface_capabilities(pdevice, surface)
                    .expect("Failed to get device surface capabilities."),
                surface_loader
                    .get_physical_device_surface_formats(pdevice, surface)
                    .expect("Failed to get device surface formats."),
                surface_loader
                    .get_physical_device_surface_present_modes(pdevice, surface)
                    .expect("Failed to get device surface modes."),
            )
        }
    }

    fn basic(
        entry: Entry,
        instance: Instance,
        window: Window,
        device: Device,
        surface_loader: surface::Instance,
        swap_device: swapchain::Device,
    ) -> Self {
        App {
            entry,
            instance,
            window,
            pdevice: vk::PhysicalDevice::default(),
            device,
            queue: vk::Queue::default(),
            surface: vk::SurfaceKHR::default(),
            surface_loader,
            swapchain: vk::SwapchainKHR::default(),
            swap_device,
            swap_imgs: Vec::<vk::Image>::default(),
            swap_img_views: Vec::<vk::ImageView>::default(),
            swap_framebuffers: Vec::<vk::Framebuffer>::default(),
            format: vk::SurfaceFormatKHR::default(),
            extent: vk::Extent2D::default(),
            pipeline_layout: vk::PipelineLayout::default(),
            render_pass: vk::RenderPass::default(),
            pipeline: vk::Pipeline::default(),
            command_pool: vk::CommandPool::default(),
            command_buffers: [vk::CommandBuffer::default(); MAX_IN_FLIGHT],
            image_available: [vk::Semaphore::default(); MAX_IN_FLIGHT],
            render_done: [vk::Semaphore::default(); MAX_IN_FLIGHT],
            in_flight: [vk::Fence::default(); MAX_IN_FLIGHT],
            cur_frame: 0,
        }
    }
}

impl ApplicationHandler for WrappedApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
                    .with_title("gaem"),
            )
            .unwrap();
        let entry = ash::Entry::linked();
        let instance = {
            let layer_names = [c"VK_LAYER_KHRONOS_validation"];
            let layer_names_raw: Vec<*const c_char> = layer_names
                .iter()
                .map(|raw_name| raw_name.as_ptr())
                .collect();
            let app_info = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 0, 0))
                .application_name(c"gaem")
                .application_version(0)
                .engine_name(c"gaem")
                .engine_version(0);
            let mut extension_names = ash_window::enumerate_required_extensions(
                window.display_handle().unwrap().as_raw(),
            )
            .expect("Failed to enumerate required extensions.")
            .to_vec();
            extension_names.push(debug_utils::NAME.as_ptr());
            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_layer_names(&layer_names_raw)
                .enabled_extension_names(&extension_names)
                .flags(vk::InstanceCreateFlags::default());
            unsafe { entry.create_instance(&instance_info, None).unwrap() }
        };
        let extension_names = [swapchain::NAME];
        let extension_names_raw = [swapchain::NAME.as_ptr()];
        let check_dev_props_valid = |props: &Vec<vk::ExtensionProperties>| {
            extension_names.iter().all(|e| {
                props
                    .into_iter()
                    .map(|p| p.extension_name_as_c_str().unwrap())
                    .collect::<Vec<&CStr>>()
                    .contains(e)
            })
        };
        let surface_loader = surface::Instance::new(&entry, &instance);
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )
            .unwrap()
        };
        let (queue_ind, pdevice) = unsafe {
            let check_device = |d: vk::PhysicalDevice, i: u32, info: &vk::QueueFamilyProperties| {
                info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && surface_loader
                            .get_physical_device_surface_support(d, i, surface)
                            .unwrap_or(false)
                        && instance
                            .enumerate_device_extension_properties(d)
                            .map(|props| check_dev_props_valid(&props))
                            .unwrap_or(false)
                        // only query support after verifying extensions
                        && {
                            let (_, formats, modes) = App::get_swap_support(d, &surface_loader, surface);
                            !formats.is_empty() && !modes.is_empty()
                        }
            };
            instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate devices.")
                .iter()
                .find_map(|d| {
                    instance
                        .get_physical_device_queue_family_properties(*d)
                        .iter()
                        .enumerate()
                        .find_map(|(i, info)| {
                            check_device(*d, i as u32, info).then(|| (i as u32, *d))
                        })
                })
                .expect("Failed to find suitable device")
        };
        let device = unsafe {
            let q_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_priorities(&[1.0])
                .queue_family_index(queue_ind)];
            let features = instance.get_physical_device_features(pdevice);
            let device_info = vk::DeviceCreateInfo::default()
                .enabled_features(&features)
                .enabled_extension_names(&extension_names_raw)
                .queue_create_infos(&q_infos);
            instance
                .create_device(pdevice, &device_info, None)
                .expect("Failed to create device.")
        };
        let swap_device = swapchain::Device::new(&instance, &device);
        let mut app = App::basic(entry, instance, window, device, surface_loader, swap_device);
        app.pdevice = pdevice;
        app.surface = surface;
        app.create_swapchain();

        unsafe {
            app.queue = app.device.get_device_queue(queue_ind, 0);
            let shader_code = read_spv(&mut Cursor::new(&include_bytes!("../../shaders/target/spirv-builder/spirv-unknown-spv1.0/release/deps/shader_crate.spv")[..])).expect("Failed to read shader spv.");
            let info = vk::ShaderModuleCreateInfo::default().code(&shader_code);
            let shader_module = app
                .device
                .create_shader_module(&info, None)
                .expect("Failed to create shader module.");
            let shader_stage = |name, flags| {
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(flags)
                    .module(shader_module)
                    .name(name)
            };
            let shader_stage_info = [
                shader_stage(c"vert_main", vk::ShaderStageFlags::VERTEX),
                shader_stage(c"frag_main", vk::ShaderStageFlags::FRAGMENT),
            ];
            let vert_in_info = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&[])
                .vertex_attribute_descriptions(&[]);
            let dyn_states = vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dyn_state_info =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyn_states);
            let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false);
            let viewport_info = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
                .depth_clamp_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::BACK)
                .front_face(vk::FrontFace::CLOCKWISE)
                .depth_bias_enable(false);
            let multisampling_info = vk::PipelineMultisampleStateCreateInfo::default()
                .sample_shading_enable(false)
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let blending_attachment = [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)];
            let blending_info = vk::PipelineColorBlendStateCreateInfo::default()
                .logic_op_enable(false)
                .attachments(&blending_attachment);
            let layout_info = vk::PipelineLayoutCreateInfo::default();
            app.pipeline_layout = app
                .device
                .create_pipeline_layout(&layout_info, None)
                .expect("Failed to create pipeline layout.");
            let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stage_info)
                .vertex_input_state(&vert_in_info)
                .input_assembly_state(&input_assembly_info)
                .viewport_state(&viewport_info)
                .rasterization_state(&rasterizer_info)
                .multisample_state(&multisampling_info)
                .color_blend_state(&blending_info)
                .dynamic_state(&dyn_state_info)
                .layout(app.pipeline_layout)
                .render_pass(app.render_pass)
                .subpass(0)];
            app.pipeline = app
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("Failed to create graphics pipeline.")[0];
            app.device.destroy_shader_module(shader_module, None);

            let pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_ind);
            app.command_pool = app
                .device
                .create_command_pool(&pool_info, None)
                .expect("Failed to create command pool.");
            let buff_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(app.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            for i in 0..MAX_IN_FLIGHT {
                app.command_buffers[i] = app
                    .device
                    .allocate_command_buffers(&buff_info)
                    .expect("Failed to allocate command buffers")[0]
            }

            let fence_create_info =
                vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            for i in 0..MAX_IN_FLIGHT {
                app.image_available[i] = app
                    .device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .expect("Failed to create semaphore.");
                app.render_done[i] = app
                    .device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .expect("Failed to create semaphore.");
                app.in_flight[i] = app
                    .device
                    .create_fence(&fence_create_info, None)
                    .expect("Failed to create fence.");
            }
        };

        *self = WrappedApp(Some(app));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let Some(ref mut app) = self.0 {
            match event {
                WindowEvent::Destroyed | WindowEvent::CloseRequested => {
                    unsafe { app.device.device_wait_idle().unwrap() };
                    event_loop.exit()
                }
                WindowEvent::RedrawRequested => app.render(),
                _ => println!("Unhandled event {:?}", event),
            }
        } else {
            println!("None window received and ignored event {:?}.", event);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            for i in 0..MAX_IN_FLIGHT {
                self.device.destroy_semaphore(self.render_done[i], None);
                self.device.destroy_semaphore(self.image_available[i], None);
                self.device.destroy_fence(self.in_flight[i], None);
            }
            self.clean_swapchain();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}
