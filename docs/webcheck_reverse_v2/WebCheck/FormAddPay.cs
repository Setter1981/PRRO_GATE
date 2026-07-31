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
public class FormAddPay : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	private string IdPay;

	private string NamePay;

	private string GrPay;

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

	[field: AccessedThroughProperty("NamePayT")]
	internal virtual TextBox NamePayT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label8")]
	internal virtual Label Label8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("RB2")]
	internal virtual RadioButton RB2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("RB1")]
	internal virtual RadioButton RB1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("RB0")]
	internal virtual RadioButton RB0
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabooL")]
	internal virtual Label TabooL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormAddPay));
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		this.NamePayT = new System.Windows.Forms.TextBox();
		this.Label8 = new System.Windows.Forms.Label();
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.RB2 = new System.Windows.Forms.RadioButton();
		this.RB1 = new System.Windows.Forms.RadioButton();
		this.RB0 = new System.Windows.Forms.RadioButton();
		this.TabooL = new System.Windows.Forms.Label();
		this.GroupBox1.SuspendLayout();
		base.SuspendLayout();
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(17, 160);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(196, 40);
		this.NoB.TabIndex = 42;
		this.NoB.TabStop = false;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(619, 160);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(196, 40);
		this.OkB.TabIndex = 41;
		this.OkB.TabStop = false;
		this.OkB.Text = "Ок";
		this.OkB.UseVisualStyleBackColor = true;
		this.NamePayT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NamePayT.Location = new System.Drawing.Point(194, 42);
		this.NamePayT.Name = "NamePayT";
		this.NamePayT.Size = new System.Drawing.Size(621, 30);
		this.NamePayT.TabIndex = 1;
		this.NamePayT.TabStop = false;
		this.NamePayT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label8.AutoSize = true;
		this.Label8.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label8.Location = new System.Drawing.Point(30, 45);
		this.Label8.Name = "Label8";
		this.Label8.Size = new System.Drawing.Size(145, 25);
		this.Label8.TabIndex = 39;
		this.Label8.Text = "Засіб оплати *";
		this.GroupBox1.Controls.Add(this.RB2);
		this.GroupBox1.Controls.Add(this.RB1);
		this.GroupBox1.Controls.Add(this.RB0);
		this.GroupBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox1.Location = new System.Drawing.Point(17, 83);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Size = new System.Drawing.Size(798, 60);
		this.GroupBox1.TabIndex = 46;
		this.GroupBox1.TabStop = false;
		this.GroupBox1.Text = "Форма оплати";
		this.RB2.AutoSize = true;
		this.RB2.Location = new System.Drawing.Point(671, 22);
		this.RB2.Name = "RB2";
		this.RB2.Size = new System.Drawing.Size(76, 29);
		this.RB2.TabIndex = 48;
		this.RB2.Text = "Інше";
		this.RB2.UseVisualStyleBackColor = true;
		this.RB1.AutoSize = true;
		this.RB1.Location = new System.Drawing.Point(453, 22);
		this.RB1.Name = "RB1";
		this.RB1.Size = new System.Drawing.Size(132, 29);
		this.RB1.TabIndex = 47;
		this.RB1.Text = "Безготівка";
		this.RB1.UseVisualStyleBackColor = true;
		this.RB0.AutoSize = true;
		this.RB0.Location = new System.Drawing.Point(269, 22);
		this.RB0.Name = "RB0";
		this.RB0.Size = new System.Drawing.Size(102, 29);
		this.RB0.TabIndex = 46;
		this.RB0.Text = "Готівка";
		this.RB0.UseVisualStyleBackColor = true;
		this.TabooL.AutoSize = true;
		this.TabooL.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TabooL.ForeColor = System.Drawing.Color.DarkRed;
		this.TabooL.Location = new System.Drawing.Point(218, 10);
		this.TabooL.Name = "TabooL";
		this.TabooL.Size = new System.Drawing.Size(528, 20);
		this.TabooL.TabIndex = 47;
		this.TabooL.Text = "Редагування напередвизначених  засобів оплат заборонено!";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(835, 213);
		base.Controls.Add(this.TabooL);
		base.Controls.Add(this.GroupBox1);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.NamePayT);
		base.Controls.Add(this.Label8);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormAddPay";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormAddPay(string eIdPay = "", string eNamePay = "", string eGrPay = "")
	{
		base.Load += FormAddPay_Load;
		InitializeComponent();
		IdPay = eIdPay;
		NamePay = eNamePay;
		GrPay = eGrPay;
		if (IdPay.Length > 0)
		{
			Text = "Редагування платежу № " + IdPay;
			NamePayT.Text = NamePay;
			if (Operators.CompareString(GrPay.ToLower(), "готівка", TextCompare: false) == 0)
			{
				RB0.Checked = true;
			}
			else if (Operators.CompareString(GrPay.ToLower(), "інше", TextCompare: false) == 0)
			{
				RB2.Checked = true;
			}
			else
			{
				RB1.Checked = true;
			}
		}
		else
		{
			Text = "Новий засіб оплати";
			NamePayT.Text = "";
			RB1.Checked = true;
		}
	}

	private void FormAddPay_Load(object sender, EventArgs e)
	{
		base.AcceptButton = OkB;
		base.CancelButton = NoB;
		RB0.Enabled = false;
		RB2.Enabled = false;
		if (Versioned.IsNumeric(IdPay))
		{
			if (Conversions.ToInteger(IdPay) < 5)
			{
				TabooL.Visible = true;
				RB0.Enabled = false;
				RB1.Enabled = false;
				RB2.Enabled = false;
				NamePayT.Enabled = false;
			}
			else
			{
				TabooL.Visible = false;
			}
		}
		else
		{
			TabooL.Visible = false;
			Show();
			NamePayT.Focus();
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		TypErr typErr = default(TypErr);
		typErr.errStr = "";
		typErr.errCode = 0;
		string text = All.A.FN;
		if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0)
		{
			text += "_TS";
		}
		CreateDB createDB = new CreateDB(text);
		if (Versioned.IsNumeric(IdPay))
		{
			if (Conversions.ToInteger(IdPay) < 5)
			{
				Close();
				return;
			}
			if (Operators.CompareString(NamePayT.Text.Trim(), "", TextCompare: false) == 0)
			{
				NamePayT.Focus();
				return;
			}
			if (Operators.CompareString(All.l.ReturnOpenShift().ReturnStr, "0", TextCompare: false) != 0)
			{
				Interaction.MsgBox("Закрийте зміну!", MsgBoxStyle.Exclamation, "Помилка");
				All.Lg.SaveTextToLog("PayForm", "Неможливо редагувати засіб оплати у відкритій зміні");
				Close();
				return;
			}
			if (All.ForbiddenSymbols(NamePayT.Text.Trim()).errCode > 0)
			{
				Interaction.MsgBox("У назві використовується помилковий символ!", MsgBoxStyle.Exclamation, "Помилка");
				All.Lg.SaveTextToLog("PayForm", "Для спеціальних символів (апостроф, лапка) в назві засоба оплати використовуйте заміну на HTML символи. " + NamePayT.Text.Trim());
				Close();
				return;
			}
			typErr = createDB.UpdatePayForms(NamePayT.Text.Trim(), NamePay, FP(), IdPay);
			if (typErr.errCode != 0)
			{
				Interaction.MsgBox("Швидше за все, такий платіж вже є!", MsgBoxStyle.Exclamation, "Помилка");
				All.Lg.SaveTextToLog("PayForm", typErr.errStr);
			}
			Close();
		}
		else if (Operators.CompareString(NamePayT.Text.Trim(), "", TextCompare: false) == 0)
		{
			NamePayT.Focus();
		}
		else if (All.ForbiddenSymbols(NamePayT.Text.Trim()).errCode > 0)
		{
			Interaction.MsgBox("У назві використовується помилковий символ!", MsgBoxStyle.Exclamation, "Помилка");
			All.Lg.SaveTextToLog("PayForm", "Для спеціальних символів (апостроф, лапка) в назві засоба оплати використовуйте заміну на HTML символи. " + NamePayT.Text.Trim());
			Close();
		}
		else
		{
			typErr = createDB.SavePayForms(NamePayT.Text.Trim(), FP());
			if (typErr.errCode != 0)
			{
				Interaction.MsgBox("Швидше за все, такий платіж вже є!", MsgBoxStyle.Exclamation, "Помилка");
				All.Lg.SaveTextToLog("PayForm", typErr.errStr);
			}
			Close();
		}
	}

	private string FP()
	{
		if (RB0.Checked)
		{
			return "1";
		}
		if (RB2.Checked)
		{
			return "102";
		}
		return "0";
	}
}
