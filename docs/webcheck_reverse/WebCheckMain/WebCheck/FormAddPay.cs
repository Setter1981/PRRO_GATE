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
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_009b: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a5: Expected O, but got Unknown
		//IL_012f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0139: Expected O, but got Unknown
		//IL_01c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_01d0: Expected O, but got Unknown
		//IL_0255: Unknown result type (might be due to invalid IL or missing references)
		//IL_025f: Expected O, but got Unknown
		//IL_0310: Unknown result type (might be due to invalid IL or missing references)
		//IL_031a: Expected O, but got Unknown
		//IL_04f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_04fe: Expected O, but got Unknown
		//IL_0613: Unknown result type (might be due to invalid IL or missing references)
		//IL_061d: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormAddPay));
		NoB = new Button();
		OkB = new Button();
		NamePayT = new TextBox();
		Label8 = new Label();
		GroupBox1 = new GroupBox();
		RB2 = new RadioButton();
		RB1 = new RadioButton();
		RB0 = new RadioButton();
		TabooL = new Label();
		((Control)GroupBox1).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(17, 160);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(196, 40);
		((Control)NoB).TabIndex = 42;
		((Control)NoB).TabStop = false;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(619, 160);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(196, 40);
		((Control)OkB).TabIndex = 41;
		((Control)OkB).TabStop = false;
		((ButtonBase)OkB).Text = "Ок";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)NamePayT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NamePayT).Location = new Point(194, 42);
		((Control)NamePayT).Name = "NamePayT";
		((Control)NamePayT).Size = new Size(621, 30);
		((Control)NamePayT).TabIndex = 1;
		((Control)NamePayT).TabStop = false;
		NamePayT.TextAlign = (HorizontalAlignment)2;
		Label8.AutoSize = true;
		((Control)Label8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label8).Location = new Point(30, 45);
		((Control)Label8).Name = "Label8";
		((Control)Label8).Size = new Size(145, 25);
		((Control)Label8).TabIndex = 39;
		Label8.Text = "Засіб оплати *";
		((Control)GroupBox1).Controls.Add((Control)(object)RB2);
		((Control)GroupBox1).Controls.Add((Control)(object)RB1);
		((Control)GroupBox1).Controls.Add((Control)(object)RB0);
		((Control)GroupBox1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox1).Location = new Point(17, 83);
		((Control)GroupBox1).Name = "GroupBox1";
		((Control)GroupBox1).Size = new Size(798, 60);
		((Control)GroupBox1).TabIndex = 46;
		GroupBox1.TabStop = false;
		GroupBox1.Text = "Форма оплати";
		((ButtonBase)RB2).AutoSize = true;
		((Control)RB2).Location = new Point(671, 22);
		((Control)RB2).Name = "RB2";
		((Control)RB2).Size = new Size(76, 29);
		((Control)RB2).TabIndex = 48;
		((ButtonBase)RB2).Text = "Інше";
		((ButtonBase)RB2).UseVisualStyleBackColor = true;
		((ButtonBase)RB1).AutoSize = true;
		((Control)RB1).Location = new Point(453, 22);
		((Control)RB1).Name = "RB1";
		((Control)RB1).Size = new Size(132, 29);
		((Control)RB1).TabIndex = 47;
		((ButtonBase)RB1).Text = "Безготівка";
		((ButtonBase)RB1).UseVisualStyleBackColor = true;
		((ButtonBase)RB0).AutoSize = true;
		((Control)RB0).Location = new Point(269, 22);
		((Control)RB0).Name = "RB0";
		((Control)RB0).Size = new Size(102, 29);
		((Control)RB0).TabIndex = 46;
		((ButtonBase)RB0).Text = "Готівка";
		((ButtonBase)RB0).UseVisualStyleBackColor = true;
		TabooL.AutoSize = true;
		((Control)TabooL).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TabooL).ForeColor = Color.DarkRed;
		((Control)TabooL).Location = new Point(218, 10);
		((Control)TabooL).Name = "TabooL";
		((Control)TabooL).Size = new Size(528, 20);
		((Control)TabooL).TabIndex = 47;
		TabooL.Text = "Редагування напередвизначених  засобів оплат заборонено!";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(835, 213);
		((Control)this).Controls.Add((Control)(object)TabooL);
		((Control)this).Controls.Add((Control)(object)GroupBox1);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)NamePayT);
		((Control)this).Controls.Add((Control)(object)Label8);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormAddPay";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Control)GroupBox1).ResumeLayout(false);
		((Control)GroupBox1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormAddPay(string eIdPay = "", string eNamePay = "", string eGrPay = "")
	{
		((Form)this).Load += FormAddPay_Load;
		InitializeComponent();
		IdPay = eIdPay;
		NamePay = eNamePay;
		GrPay = eGrPay;
		if (IdPay.Length > 0)
		{
			((Form)this).Text = "Редагування платежу № " + IdPay;
			NamePayT.Text = NamePay;
			if (Operators.CompareString(GrPay.ToLower(), "готівка", false) == 0)
			{
				RB0.Checked = true;
			}
			else if (Operators.CompareString(GrPay.ToLower(), "інше", false) == 0)
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
			((Form)this).Text = "Новий засіб оплати";
			NamePayT.Text = "";
			RB1.Checked = true;
		}
	}

	private void FormAddPay_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
		((Control)RB0).Enabled = false;
		((Control)RB2).Enabled = false;
		if (Versioned.IsNumeric((object)IdPay))
		{
			if (Conversions.ToInteger(IdPay) < 5)
			{
				((Control)TabooL).Visible = true;
				((Control)RB0).Enabled = false;
				((Control)RB1).Enabled = false;
				((Control)RB2).Enabled = false;
				((Control)NamePayT).Enabled = false;
			}
			else
			{
				((Control)TabooL).Visible = false;
			}
		}
		else
		{
			((Control)TabooL).Visible = false;
			((Control)this).Show();
			((Control)NamePayT).Focus();
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		//IL_0201: Unknown result type (might be due to invalid IL or missing references)
		//IL_026d: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Unknown result type (might be due to invalid IL or missing references)
		//IL_010f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0187: Unknown result type (might be due to invalid IL or missing references)
		TypErr typErr = default(TypErr);
		typErr.errStr = "";
		typErr.errCode = 0;
		string text = All.A.FN;
		if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0)
		{
			text += "_TS";
		}
		CreateDB createDB = new CreateDB(text);
		if (Versioned.IsNumeric((object)IdPay))
		{
			if (Conversions.ToInteger(IdPay) < 5)
			{
				((Form)this).Close();
				return;
			}
			if (Operators.CompareString(NamePayT.Text.Trim(), "", false) == 0)
			{
				((Control)NamePayT).Focus();
				return;
			}
			if (Operators.CompareString(All.l.ReturnOpenShift().ReturnStr, "0", false) != 0)
			{
				Interaction.MsgBox((object)"Закрийте зміну!", (MsgBoxStyle)48, (object)"Помилка");
				All.Lg.SaveTextToLog("PayForm", "Неможливо редагувати засіб оплати у відкритій зміні");
				((Form)this).Close();
				return;
			}
			if (All.ForbiddenSymbols(NamePayT.Text.Trim()).errCode > 0)
			{
				Interaction.MsgBox((object)"У назві використовується помилковий символ!", (MsgBoxStyle)48, (object)"Помилка");
				All.Lg.SaveTextToLog("PayForm", "Для спеціальних символів (апостроф, лапка) в назві засоба оплати використовуйте заміну на HTML символи. " + NamePayT.Text.Trim());
				((Form)this).Close();
				return;
			}
			typErr = createDB.UpdatePayForms(NamePayT.Text.Trim(), NamePay, FP(), IdPay);
			if (typErr.errCode != 0)
			{
				Interaction.MsgBox((object)"Швидше за все, такий платіж вже є!", (MsgBoxStyle)48, (object)"Помилка");
				All.Lg.SaveTextToLog("PayForm", typErr.errStr);
			}
			((Form)this).Close();
		}
		else if (Operators.CompareString(NamePayT.Text.Trim(), "", false) == 0)
		{
			((Control)NamePayT).Focus();
		}
		else if (All.ForbiddenSymbols(NamePayT.Text.Trim()).errCode > 0)
		{
			Interaction.MsgBox((object)"У назві використовується помилковий символ!", (MsgBoxStyle)48, (object)"Помилка");
			All.Lg.SaveTextToLog("PayForm", "Для спеціальних символів (апостроф, лапка) в назві засоба оплати використовуйте заміну на HTML символи. " + NamePayT.Text.Trim());
			((Form)this).Close();
		}
		else
		{
			typErr = createDB.SavePayForms(NamePayT.Text.Trim(), FP());
			if (typErr.errCode != 0)
			{
				Interaction.MsgBox((object)"Швидше за все, такий платіж вже є!", (MsgBoxStyle)48, (object)"Помилка");
				All.Lg.SaveTextToLog("PayForm", typErr.errStr);
			}
			((Form)this).Close();
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
