using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form0 : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("AddFn")]
	private Button _AddFn;

	[CompilerGenerated]
	[AccessedThroughProperty("LinkLabel1")]
	private LinkLabel _LinkLabel1;

	internal virtual Button AddFn
	{
		[CompilerGenerated]
		get
		{
			return _AddFn;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = AddFn_Click;
			Button addFn = _AddFn;
			if (addFn != null)
			{
				((Control)addFn).Click -= eventHandler;
			}
			_AddFn = value;
			addFn = _AddFn;
			if (addFn != null)
			{
				((Control)addFn).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("v")]
	internal virtual Label v
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual LinkLabel LinkLabel1
	{
		[CompilerGenerated]
		get
		{
			return _LinkLabel1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			LinkLabelLinkClickedEventHandler val = new LinkLabelLinkClickedEventHandler(LinkLabel1_LinkClicked);
			LinkLabel linkLabel = _LinkLabel1;
			if (linkLabel != null)
			{
				linkLabel.LinkClicked -= val;
			}
			_LinkLabel1 = value;
			linkLabel = _LinkLabel1;
			if (linkLabel != null)
			{
				linkLabel.LinkClicked += val;
			}
		}
	}

	[field: AccessedThroughProperty("vD")]
	internal virtual Label vD
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public Form0()
	{
		((Form)this).Load += Form0_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_006f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0079: Expected O, but got Unknown
		//IL_009a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0116: Unknown result type (might be due to invalid IL or missing references)
		//IL_0120: Expected O, but got Unknown
		//IL_013e: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ba: Unknown result type (might be due to invalid IL or missing references)
		//IL_01c4: Expected O, but got Unknown
		//IL_01e8: Unknown result type (might be due to invalid IL or missing references)
		//IL_0264: Unknown result type (might be due to invalid IL or missing references)
		//IL_026e: Expected O, but got Unknown
		//IL_028f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0308: Unknown result type (might be due to invalid IL or missing references)
		//IL_0312: Expected O, but got Unknown
		//IL_0336: Unknown result type (might be due to invalid IL or missing references)
		//IL_03b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_03bd: Expected O, but got Unknown
		//IL_03de: Unknown result type (might be due to invalid IL or missing references)
		//IL_04df: Unknown result type (might be due to invalid IL or missing references)
		//IL_04e9: Expected O, but got Unknown
		//IL_04eb: Unknown result type (might be due to invalid IL or missing references)
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form0));
		AddFn = new Button();
		Label1 = new Label();
		Label2 = new Label();
		v = new Label();
		LinkLabel1 = new LinkLabel();
		vD = new Label();
		((Control)this).SuspendLayout();
		((Control)AddFn).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)AddFn).Location = new Point(212, 248);
		((Control)AddFn).Margin = new Padding(4);
		((Control)AddFn).Name = "AddFn";
		((Control)AddFn).Size = new Size(141, 34);
		((Control)AddFn).TabIndex = 3;
		((ButtonBase)AddFn).Text = "Ок";
		((ButtonBase)AddFn).UseVisualStyleBackColor = true;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(37, 18);
		((Control)Label1).Margin = new Padding(4, 0, 4, 0);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(474, 25);
		((Control)Label1).TabIndex = 4;
		Label1.Text = "Програмний Реєстратор Розрахункових Операцій";
		Label1.TextAlign = (ContentAlignment)2;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 14.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(186, 195);
		((Control)Label2).Margin = new Padding(4, 0, 4, 0);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(195, 29);
		((Control)Label2).TabIndex = 5;
		Label2.Text = "тел. 0 800 50143";
		Label2.TextAlign = (ContentAlignment)2;
		v.AutoSize = true;
		((Control)v).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)v).Location = new Point(147, 105);
		((Control)v).Margin = new Padding(4, 0, 4, 0);
		((Control)v).Name = "v";
		((Control)v).Size = new Size(78, 25);
		((Control)v).TabIndex = 7;
		v.Text = "Версия";
		v.TextAlign = (ContentAlignment)2;
		((Label)LinkLabel1).AutoSize = true;
		((Control)LinkLabel1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)LinkLabel1).Location = new Point(147, 150);
		((Control)LinkLabel1).Margin = new Padding(4, 0, 4, 0);
		((Control)LinkLabel1).Name = "LinkLabel1";
		((Control)LinkLabel1).Size = new Size(272, 25);
		((Control)LinkLabel1).TabIndex = 9;
		LinkLabel1.TabStop = true;
		LinkLabel1.Text = "https://www.webchek.com.ua/";
		vD.AutoSize = true;
		((Control)vD).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)vD).Location = new Point(186, 64);
		((Control)vD).Margin = new Padding(4, 0, 4, 0);
		((Control)vD).Name = "vD";
		((Control)vD).Size = new Size(119, 25);
		((Control)vD).TabIndex = 10;
		vD.Text = "Версия DLL";
		vD.TextAlign = (ContentAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(581, 305);
		((Control)this).Controls.Add((Control)(object)vD);
		((Control)this).Controls.Add((Control)(object)LinkLabel1);
		((Control)this).Controls.Add((Control)(object)v);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)AddFn);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).Margin = new Padding(4);
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "Form0";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "ВебЧЕК ПРРО";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void AddFn_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void Form0_Load(object sender, EventArgs e)
	{
		vD.Text = "Версія DLL: " + Vdll();
		v.Text = "Версія програми: " + Application.ProductVersion;
	}

	private void LinkLabel1_LinkClicked(object sender, LinkLabelLinkClickedEventArgs e)
	{
		try
		{
			Process.Start(new Uri("https://www.webchek.com.ua/").ToString());
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private string Vdll()
	{
		return FileVersionInfo.GetVersionInfo(FileSystem.CurDir() + "\\WebCheck.dll").FileVersion;
	}
}
