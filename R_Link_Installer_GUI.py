import tkinter as tk
from tkinter import messagebox
import ttkbootstrap as ttk
from ttkbootstrap.constants import *
from ttkbootstrap.toast import ToastNotification
import pyperclip
import re
import urllib.parse

class RLinkInstallerApp:
    def __init__(self, root):
        self.root = root
        self.root.title("R 包安装链接生成器 v1.1")
        self.root.geometry("750x500")
        self.center_window()
        
        # 设置主题
        self.style = ttk.Style(theme="darkly")
        
        # 变量
        self.url_var = tk.StringVar()
        self.method_var = tk.StringVar(value="devtools")
        self.result_var = tk.StringVar()
        self.status_var = tk.StringVar(value="等待输入...")
        
        # 绑定追踪
        self.url_var.trace_add("write", self.on_url_change)
        self.method_var.trace_add("write", self.generate_command)
        
        self.create_widgets()

    def center_window(self):
        """将窗口置于屏幕中央"""
        self.root.update_idletasks()
        width = self.root.winfo_width()
        height = self.root.winfo_height()
        x = (self.root.winfo_screenwidth() // 2) - (width // 2)
        y = (self.root.winfo_screenheight() // 2) - (height // 2)
        self.root.geometry(f"{width}x{height}+{x}+{y}")
        
    def create_widgets(self):
        # 主容器
        main_frame = ttk.Frame(self.root)
        main_frame.pack(fill=BOTH, expand=YES, padx=20, pady=20)
        
        # 标题
        title_label = ttk.Label(
            main_frame, 
            text="R 包安装链接生成器", 
            font=("Microsoft YaHei", 20, "bold"),
            bootstyle=INFO
        )
        title_label.pack(pady=(0, 20))
        
        # --- 输入区域 ---
        input_group = ttk.Labelframe(main_frame, text=" 链接输入 ")
        input_group.pack(fill=X, pady=10)
        
        entry_frame = ttk.Frame(input_group)
        entry_frame.pack(fill=X, expand=YES, padx=10, pady=10)
        
        self.url_entry = ttk.Entry(
            entry_frame, 
            textvariable=self.url_var, 
            font=("Consolas", 11)
        )
        self.url_entry.pack(side=LEFT, fill=X, expand=YES, padx=(0, 10))
        
        # 按钮容器
        btn_container = ttk.Frame(entry_frame)
        btn_container.pack(side=RIGHT)

        ttk.Button(
            btn_container, 
            text="粘贴", 
            command=self.paste_url,
            bootstyle=SECONDARY,
            width=8
        ).pack(side=LEFT, padx=2)

        ttk.Button(
            btn_container, 
            text="清空", 
            command=self.clear_url,
            bootstyle=DANGER,
            width=8
        ).pack(side=LEFT, padx=2)
        
        # --- 选项区域 ---
        options_group = ttk.Labelframe(main_frame, text=" 安装工具选择 ")
        options_group.pack(fill=X, pady=10)
        
        radio_frame = ttk.Frame(options_group)
        radio_frame.pack(padx=10, pady=10)
        
        ttk.Radiobutton(
            radio_frame, 
            text="devtools::install_url", 
            value="devtools", 
            variable=self.method_var,
            bootstyle="info-toolbutton"
        ) .pack(side=LEFT, padx=5)
        
        ttk.Radiobutton(
            radio_frame, 
            text="remotes::install_url", 
            value="remotes", 
            variable=self.method_var,
            bootstyle="info-toolbutton"
        ) .pack(side=LEFT, padx=5)

        ttk.Radiobutton(
            radio_frame, 
            text="install.packages", 
            value="base", 
            variable=self.method_var,
            bootstyle="info-toolbutton"
        ) .pack(side=LEFT, padx=5)

        ttk.Radiobutton(
            radio_frame, 
            text="GitHub (remotes)", 
            value="github", 
            variable=self.method_var,
            bootstyle="info-toolbutton"
        ) .pack(side=LEFT, padx=5)

        ttk.Radiobutton(
            radio_frame, 
            text="packageVersion", 
            value="version", 
            variable=self.method_var,
            bootstyle="info-toolbutton"
        ) .pack(side=LEFT, padx=5)
        
        # --- 输出区域 ---
        output_group = ttk.Labelframe(main_frame, text=" 生成结果 ")
        output_group.pack(fill=X, pady=10)
        
        output_inner = ttk.Frame(output_group)
        output_inner.pack(fill=X, padx=10, pady=10)
        
        self.result_entry = ttk.Entry(
            output_inner, 
            textvariable=self.result_var, 
            font=("Consolas", 11),
            state="readonly",
            cursor="hand2"
        )
        self.result_entry.pack(fill=X)
        self.result_entry.bind("<Button-1>", lambda e: self.copy_result())
        
        # 状态栏
        self.status_label = ttk.Label(
            main_frame,
            textvariable=self.status_var,
            font=("Microsoft YaHei", 9),
            bootstyle=SECONDARY
        )
        self.status_label.pack(pady=(5, 0))

        # --- 底部按钮 ---
        copy_btn = ttk.Button(
            main_frame, 
            text="复制生成的代码", 
            command=self.copy_result,
            bootstyle=SUCCESS
        )
        copy_btn.pack(pady=(15, 20), fill=X, ipady=5)
        
        # 初始刷新
        self.generate_command()

    def paste_url(self, *args):
        """从剪贴板粘贴 URL"""
        try:
            url = pyperclip.paste().strip()
            self.url_var.set(url)
        except Exception as e:
            messagebox.showerror("错误", f"粘贴失败: {e}")

    def clear_url(self):
        """清空输入框"""
        self.url_var.set("")
        self.result_var.set("等待输入 URL...")
        self.status_var.set("已清空")

    def on_url_change(self, *args):
        """URL 变动时的回调"""
        url = self.url_var.get().strip()
        if "github.com" in url:
            self.method_var.set("github")
            self.status_var.set("✨ 已自动识别 GitHub 链接")
        elif url == "":
            self.status_var.set("等待输入...")
        else:
            self.status_var.set("就绪")
        self.generate_command()

    def extract_package_info(self, url):
        """智能从 URL 中提取包名或 Repo 信息"""
        # 1. GitHub 模式
        github_match = re.search(r"github\.com/([^/]+/[^/]+)", url)
        if github_match:
            repo = github_match.group(1).rstrip(".git")
            # 如果包含 /tree/master 之类的路径，截断
            if "/tree/" in repo:
                repo = repo.split("/tree/")[0]
            return {"type": "github", "name": repo}

        # 2. 压缩包模式 (xxx_1.2.3.tar.gz)
        basename = url.split('/')[-1]
        pkg_name_match = re.match(r"^([a-zA-Z0-9\.]+)_[\d\.]+", basename)
        if pkg_name_match:
            return {"type": "archive", "name": pkg_name_match.group(1)}
        
        # 3. 兜底解析
        clean_name = basename.replace(".tar.gz", "").replace(".zip", "").split('_')[0]
        if not clean_name or "http" in clean_name:
            clean_name = "packageName"
        return {"type": "unknown", "name": clean_name}

    def generate_command(self, *args):
        url = self.url_var.get().strip()
        if not url:
            self.result_var.set("等待输入 URL...")
            return
            
        method = self.method_var.get()
        pkg_info = self.extract_package_info(url)

        if method == "github":
            command = f'remotes::install_github("{pkg_info["name"]}")'
        elif method == "base":
            command = f'install.packages("{url}", repos = NULL, type = "source")'
        elif method == "version":
            command = f'packageVersion("{pkg_info["name"]}")'
        else:
            # devtools 或 remotes 的 install_url
            command = f'{method}::install_url("{url}")'
            
        self.result_var.set(command)

    def copy_result(self):
        content = self.result_var.get()
        if content and "等待输入" not in content:
            try:
                pyperclip.copy(content)
                # 使用 Toast 代替弹窗
                toast = ToastNotification(
                    title="复制成功",
                    message="代码已存入剪贴板",
                    duration=2000,
                    bootstyle=SUCCESS
                )
                toast.show_toast()
            except Exception as e:
                messagebox.showerror("错误", f"复制失败: {e}")
        else:
            toast = ToastNotification(
                title="复制失败",
                message="没有有效的内容可复制",
                duration=2000,
                bootstyle=DANGER
            )
            toast.show_toast()

if __name__ == "__main__":
    root = ttk.Window()
    app = RLinkInstallerApp(root)
    root.mainloop()
