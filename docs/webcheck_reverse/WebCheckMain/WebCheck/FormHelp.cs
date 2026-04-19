using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormHelp : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("PasB")]
	private TextBox _PasB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	internal virtual TextBox PasB
	{
		[CompilerGenerated]
		get
		{
			return _PasB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PasB_TextChanged;
			TextBox pasB = _PasB;
			if (pasB != null)
			{
				((Control)pasB).TextChanged -= eventHandler;
			}
			_PasB = value;
			pasB = _PasB;
			if (pasB != null)
			{
				((Control)pasB).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TimingT")]
	internal virtual TextBox TimingT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Secret")]
	internal virtual GroupBox Secret
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormHelp()
	{
		((Form)this).Load += FormHelp_Load;
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
		//IL_007a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0084: Expected O, but got Unknown
		//IL_00fb: Unknown result type (might be due to invalid IL or missing references)
		//IL_0105: Expected O, but got Unknown
		//IL_0238: Unknown result type (might be due to invalid IL or missing references)
		//IL_0242: Expected O, but got Unknown
		//IL_02b1: Unknown result type (might be due to invalid IL or missing references)
		//IL_02bb: Expected O, but got Unknown
		//IL_0339: Unknown result type (might be due to invalid IL or missing references)
		//IL_0343: Expected O, but got Unknown
		//IL_0412: Unknown result type (might be due to invalid IL or missing references)
		//IL_041c: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormHelp));
		PasB = new TextBox();
		TimingT = new TextBox();
		Secret = new GroupBox();
		Label1 = new Label();
		NoB = new Button();
		OkB = new Button();
		((Control)Secret).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)PasB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PasB).Location = new Point(12, 12);
		((Control)PasB).Name = "PasB";
		PasB.PasswordChar = '-';
		((Control)PasB).Size = new Size(180, 30);
		((Control)PasB).TabIndex = 0;
		PasB.TextAlign = (HorizontalAlignment)2;
		((Control)TimingT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TimingT).Location = new Point(303, 38);
		((Control)TimingT).Name = "TimingT";
		((Control)TimingT).Size = new Size(180, 30);
		((Control)TimingT).TabIndex = 1;
		TimingT.TextAlign = (HorizontalAlignment)2;
		((Control)Secret).Controls.Add((Control)(object)Label1);
		((Control)Secret).Controls.Add((Control)(object)NoB);
		((Control)Secret).Controls.Add((Control)(object)OkB);
		((Control)Secret).Controls.Add((Control)(object)TimingT);
		((Control)Secret).Enabled = false;
		((Control)Secret).Location = new Point(12, 48);
		((Control)Secret).Name = "Secret";
		((Control)Secret).Size = new Size(503, 288);
		((Control)Secret).TabIndex = 2;
		Secret.TabStop = false;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(14, 43);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(204, 25);
		((Control)Label1).TabIndex = 10;
		Label1.Text = "Таймінг ПРОТО (мс)";
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(19, 229);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(217, 37);
		((Control)NoB).TabIndex = 9;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(266, 229);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(217, 37);
		((Control)OkB).TabIndex = 8;
		((ButtonBase)OkB).Text = "Застосувати ";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(528, 345);
		((Control)this).Controls.Add((Control)(object)Secret);
		((Control)this).Controls.Add((Control)(object)PasB);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormHelp";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Тестові настройки ";
		((Control)Secret).ResumeLayout(false);
		((Control)Secret).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormHelp_Load(object sender, EventArgs e)
	{
		TimingT.Text = All.Timing.ToString();
	}

	private void PasB_TextChanged(object sender, EventArgs e)
	{
		if (Operators.CompareString(PasB.Text, "2020", false) == 0)
		{
			((Control)Secret).Enabled = true;
		}
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)TimingT.Text))
		{
			All.Timing = Conversions.ToInteger(TimingT.Text);
		}
		((Form)this).Close();
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}
}
