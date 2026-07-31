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
			EventHandler value2 = PasB_TextChanged;
			TextBox pasB = _PasB;
			if (pasB != null)
			{
				pasB.TextChanged -= value2;
			}
			_PasB = value;
			pasB = _PasB;
			if (pasB != null)
			{
				pasB.TextChanged += value2;
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
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
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
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
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
		base.Load += FormHelp_Load;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormHelp));
		this.PasB = new System.Windows.Forms.TextBox();
		this.TimingT = new System.Windows.Forms.TextBox();
		this.Secret = new System.Windows.Forms.GroupBox();
		this.Label1 = new System.Windows.Forms.Label();
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		this.Secret.SuspendLayout();
		base.SuspendLayout();
		this.PasB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PasB.Location = new System.Drawing.Point(12, 12);
		this.PasB.Name = "PasB";
		this.PasB.PasswordChar = '-';
		this.PasB.Size = new System.Drawing.Size(180, 30);
		this.PasB.TabIndex = 0;
		this.PasB.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.TimingT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TimingT.Location = new System.Drawing.Point(303, 38);
		this.TimingT.Name = "TimingT";
		this.TimingT.Size = new System.Drawing.Size(180, 30);
		this.TimingT.TabIndex = 1;
		this.TimingT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Secret.Controls.Add(this.Label1);
		this.Secret.Controls.Add(this.NoB);
		this.Secret.Controls.Add(this.OkB);
		this.Secret.Controls.Add(this.TimingT);
		this.Secret.Enabled = false;
		this.Secret.Location = new System.Drawing.Point(12, 48);
		this.Secret.Name = "Secret";
		this.Secret.Size = new System.Drawing.Size(503, 288);
		this.Secret.TabIndex = 2;
		this.Secret.TabStop = false;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(14, 43);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(204, 25);
		this.Label1.TabIndex = 10;
		this.Label1.Text = "Таймінг ПРОТО (мс)";
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(19, 229);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(217, 37);
		this.NoB.TabIndex = 9;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(266, 229);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(217, 37);
		this.OkB.TabIndex = 8;
		this.OkB.Text = "Застосувати ";
		this.OkB.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(528, 345);
		base.Controls.Add(this.Secret);
		base.Controls.Add(this.PasB);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormHelp";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Тестові настройки ";
		this.Secret.ResumeLayout(false);
		this.Secret.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormHelp_Load(object sender, EventArgs e)
	{
		TimingT.Text = All.Timing.ToString();
	}

	private void PasB_TextChanged(object sender, EventArgs e)
	{
		if (Operators.CompareString(PasB.Text, "2020", TextCompare: false) == 0)
		{
			Secret.Enabled = true;
		}
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric(TimingT.Text))
		{
			All.Timing = Conversions.ToInteger(TimingT.Text);
		}
		Close();
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}
}
