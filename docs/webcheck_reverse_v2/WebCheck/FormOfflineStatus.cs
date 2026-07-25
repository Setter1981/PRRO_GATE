using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormOfflineStatus : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ExitB")]
	private Button _ExitB;

	[field: AccessedThroughProperty("ErrT")]
	internal virtual TextBox ErrT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OnT")]
	internal virtual TextBox OnT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("StartT")]
	internal virtual TextBox StartT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OstT")]
	internal virtual TextBox OstT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OstNT")]
	internal virtual TextBox OstNT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ExitB
	{
		[CompilerGenerated]
		get
		{
			return _ExitB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ExitB_Click;
			Button exitB = _ExitB;
			if (exitB != null)
			{
				exitB.Click -= value2;
			}
			_ExitB = value;
			exitB = _ExitB;
			if (exitB != null)
			{
				exitB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormOfflineStatus()
	{
		base.Load += FormOfflineStatus_Load;
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormOfflineStatus));
		this.ErrT = new System.Windows.Forms.TextBox();
		this.OnT = new System.Windows.Forms.TextBox();
		this.StartT = new System.Windows.Forms.TextBox();
		this.OstT = new System.Windows.Forms.TextBox();
		this.OstNT = new System.Windows.Forms.TextBox();
		this.ExitB = new System.Windows.Forms.Button();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		this.Label3 = new System.Windows.Forms.Label();
		this.Label4 = new System.Windows.Forms.Label();
		this.Label5 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.ErrT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ErrT.Location = new System.Drawing.Point(22, 234);
		this.ErrT.Multiline = true;
		this.ErrT.Name = "ErrT";
		this.ErrT.ReadOnly = true;
		this.ErrT.Size = new System.Drawing.Size(658, 114);
		this.ErrT.TabIndex = 0;
		this.ErrT.TabStop = false;
		this.ErrT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OnT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OnT.Location = new System.Drawing.Point(330, 15);
		this.OnT.Name = "OnT";
		this.OnT.ReadOnly = true;
		this.OnT.Size = new System.Drawing.Size(350, 30);
		this.OnT.TabIndex = 1;
		this.OnT.TabStop = false;
		this.OnT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.StartT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.StartT.Location = new System.Drawing.Point(330, 60);
		this.StartT.Name = "StartT";
		this.StartT.ReadOnly = true;
		this.StartT.Size = new System.Drawing.Size(350, 30);
		this.StartT.TabIndex = 2;
		this.StartT.TabStop = false;
		this.StartT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OstT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OstT.Location = new System.Drawing.Point(330, 107);
		this.OstT.Name = "OstT";
		this.OstT.ReadOnly = true;
		this.OstT.Size = new System.Drawing.Size(350, 30);
		this.OstT.TabIndex = 3;
		this.OstT.TabStop = false;
		this.OstT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OstNT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OstNT.Location = new System.Drawing.Point(330, 153);
		this.OstNT.Name = "OstNT";
		this.OstNT.ReadOnly = true;
		this.OstNT.Size = new System.Drawing.Size(350, 30);
		this.OstNT.TabIndex = 4;
		this.OstNT.TabStop = false;
		this.OstNT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ExitB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ExitB.Location = new System.Drawing.Point(22, 370);
		this.ExitB.Name = "ExitB";
		this.ExitB.Size = new System.Drawing.Size(658, 38);
		this.ExitB.TabIndex = 5;
		this.ExitB.Text = "Закрити ";
		this.ExitB.UseVisualStyleBackColor = true;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(17, 20);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(84, 25);
		this.Label2.TabIndex = 10;
		this.Label2.Text = "Статус:";
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(17, 65);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(195, 25);
		this.Label1.TabIndex = 11;
		this.Label1.Text = "Початок оффлайн:";
		this.Label3.AutoSize = true;
		this.Label3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label3.Location = new System.Drawing.Point(17, 112);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(263, 25);
		this.Label3.TabIndex = 12;
		this.Label3.Text = "Оффлайн номерів в черзі:";
		this.Label4.AutoSize = true;
		this.Label4.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label4.Location = new System.Drawing.Point(17, 158);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(283, 25);
		this.Label4.TabIndex = 13;
		this.Label4.Text = "Залишок резервних номерів:";
		this.Label5.AutoSize = true;
		this.Label5.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label5.Location = new System.Drawing.Point(17, 206);
		this.Label5.Name = "Label5";
		this.Label5.Size = new System.Drawing.Size(185, 25);
		this.Label5.TabIndex = 14;
		this.Label5.Text = "Остання помилка:";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(701, 424);
		base.Controls.Add(this.Label5);
		base.Controls.Add(this.Label4);
		base.Controls.Add(this.Label3);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.ExitB);
		base.Controls.Add(this.OstNT);
		base.Controls.Add(this.OstT);
		base.Controls.Add(this.StartT);
		base.Controls.Add(this.OnT);
		base.Controls.Add(this.ErrT);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormOfflineStatus";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Статус офлайн режиму ";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormOfflineStatus_Load(object sender, EventArgs e)
	{
		base.CancelButton = ExitB;
		NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
		OstNT.Text = numbersOfflineUse.CountNubmers().ToString();
		ErrT.Text = All.f.StringGetFn(All.A.FN, "LastOfflineErr");
		if (Operators.CompareString(ErrT.Text.Trim(), "", TextCompare: false) == 0)
		{
			ErrT.Text = "Відсутня";
		}
		if (All.A.FullVersion)
		{
			if (All.l.OfflineTrue())
			{
				OnT.Text = "Оффлайн";
				string text = All.l.OfflineDate().ReturnStr.Trim();
				StartT.Text = text;
				OstT.Text = All.l.OfflineCheckCount().ToString();
			}
			else
			{
				OnT.Text = "Онлайн";
				StartT.Text = "---------";
				OstT.Text = "---------";
			}
		}
		else
		{
			OnT.Text = "FREE";
			StartT.Text = "---------";
			OstT.Text = "---------";
			OstNT.Text = "---------";
			ErrT.Text = "---------";
		}
	}

	private void ExitB_Click(object sender, EventArgs e)
	{
		Close();
	}
}
