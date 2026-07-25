using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormViber : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("CheckBoxPDF")]
	private CheckBox _CheckBoxPDF;

	private string CneckTaxN;

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("telT")]
	internal virtual TextBox telT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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

	[field: AccessedThroughProperty("TextBox1")]
	internal virtual TextBox TextBox1
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

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox CheckBoxPDF
	{
		[CompilerGenerated]
		get
		{
			return _CheckBoxPDF;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CheckBoxPDF_CheckedChanged;
			CheckBox checkBoxPDF = _CheckBoxPDF;
			if (checkBoxPDF != null)
			{
				checkBoxPDF.CheckedChanged -= value2;
			}
			_CheckBoxPDF = value;
			checkBoxPDF = _CheckBoxPDF;
			if (checkBoxPDF != null)
			{
				checkBoxPDF.CheckedChanged += value2;
			}
		}
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormViber));
		this.Label2 = new System.Windows.Forms.Label();
		this.telT = new System.Windows.Forms.TextBox();
		this.OkB = new System.Windows.Forms.Button();
		this.TextBox1 = new System.Windows.Forms.TextBox();
		this.OstT = new System.Windows.Forms.TextBox();
		this.NoB = new System.Windows.Forms.Button();
		this.Label1 = new System.Windows.Forms.Label();
		this.CheckBoxPDF = new System.Windows.Forms.CheckBox();
		base.SuspendLayout();
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(13, 152);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(182, 25);
		this.Label2.TabIndex = 10;
		this.Label2.Text = "Номер телефону:";
		this.telT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.telT.Location = new System.Drawing.Point(135, 180);
		this.telT.Name = "telT";
		this.telT.Size = new System.Drawing.Size(352, 30);
		this.telT.TabIndex = 8;
		this.telT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(277, 257);
		this.OkB.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(210, 39);
		this.OkB.TabIndex = 9;
		this.OkB.Text = "Надіслати ";
		this.OkB.UseVisualStyleBackColor = true;
		this.TextBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBox1.Location = new System.Drawing.Point(18, 180);
		this.TextBox1.Name = "TextBox1";
		this.TextBox1.ReadOnly = true;
		this.TextBox1.Size = new System.Drawing.Size(102, 30);
		this.TextBox1.TabIndex = 11;
		this.TextBox1.TabStop = false;
		this.TextBox1.Text = "+38";
		this.TextBox1.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OstT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OstT.Location = new System.Drawing.Point(18, 64);
		this.OstT.Multiline = true;
		this.OstT.Name = "OstT";
		this.OstT.ReadOnly = true;
		this.OstT.Size = new System.Drawing.Size(469, 71);
		this.OstT.TabIndex = 12;
		this.OstT.TabStop = false;
		this.OstT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(18, 257);
		this.NoB.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(210, 39);
		this.NoB.TabIndex = 13;
		this.NoB.Text = "Скасувати ";
		this.NoB.UseVisualStyleBackColor = true;
		this.NoB.UseWaitCursor = true;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(13, 36);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(232, 25);
		this.Label1.TabIndex = 14;
		this.Label1.Text = "Повідомлення сервера:";
		this.CheckBoxPDF.AutoSize = true;
		this.CheckBoxPDF.Font = new System.Drawing.Font("Microsoft Sans Serif", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CheckBoxPDF.Location = new System.Drawing.Point(302, 12);
		this.CheckBoxPDF.Name = "CheckBoxPDF";
		this.CheckBoxPDF.Size = new System.Drawing.Size(185, 22);
		this.CheckBoxPDF.TabIndex = 15;
		this.CheckBoxPDF.Text = "Використовувати PDF";
		this.CheckBoxPDF.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(505, 317);
		base.Controls.Add(this.CheckBoxPDF);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OstT);
		base.Controls.Add(this.TextBox1);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.telT);
		base.Controls.Add(this.OkB);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormViber";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Надсилання чека ";
		base.TopMost = true;
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormViber(string nCh)
	{
		base.Load += FormViber_Load;
		CneckTaxN = "";
		InitializeComponent();
		CneckTaxN = nCh;
	}

	private void FormViber_Load(object sender, EventArgs e)
	{
		base.AcceptButton = OkB;
		base.CancelButton = NoB;
		InViber inViber = new InViber();
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		typErr = inViber.InTextViber();
		if (typErr.errCode > 0)
		{
			OstT.Text = typErr.errStr;
			OkB.Enabled = false;
		}
		else if (Versioned.IsNumeric(typErr.errStr))
		{
			OstT.Text = "Залишок відправок: " + typErr.errStr;
			if (Conversions.ToInteger(typErr.errStr) > 0)
			{
				OkB.Enabled = true;
			}
			else
			{
				OkB.Enabled = false;
			}
		}
		else
		{
			OstT.Text = "Помилка!";
			OkB.Enabled = false;
		}
		if (Operators.CompareString(All.f.StringGetFn(All.A.FN, "PDF"), "1", TextCompare: false) == 0)
		{
			CheckBoxPDF.Checked = true;
		}
		else
		{
			CheckBoxPDF.Checked = false;
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		OkB.Enabled = false;
		string text = "38";
		if (telT.Text.Trim().Length != 10)
		{
			OstT.Text = "Вкажіть правильний номер телефону";
			telT.Focus();
			OkB.Enabled = true;
			return;
		}
		if (!Versioned.IsNumeric(telT.Text.Trim()))
		{
			OstT.Text = "Вкажіть правильний номер телефону";
			telT.Focus();
			OkB.Enabled = true;
			return;
		}
		text += telT.Text.Trim();
		InViber inViber = new InViber();
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		typErr = inViber.InTextViber(CneckTaxN, text, 3);
		if (typErr.errCode > 0)
		{
			OstT.Text = typErr.errStr;
		}
		else
		{
			OstT.Text = "Повідомлення успішно поставлено в чергу. Залишок відправок: " + typErr.errStr;
		}
		OkB.Enabled = true;
	}

	private void CheckBoxPDF_CheckedChanged(object sender, EventArgs e)
	{
		if (CheckBoxPDF.Checked)
		{
			All.f.StringWriteFN(All.A.FN, "PDF", "1");
		}
		else
		{
			All.f.StringWriteFN(All.A.FN, "PDF", "0");
		}
	}
}
