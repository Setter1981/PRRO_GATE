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
			EventHandler eventHandler = CheckBoxPDF_CheckedChanged;
			CheckBox checkBoxPDF = _CheckBoxPDF;
			if (checkBoxPDF != null)
			{
				checkBoxPDF.CheckedChanged -= eventHandler;
			}
			_CheckBoxPDF = value;
			checkBoxPDF = _CheckBoxPDF;
			if (checkBoxPDF != null)
			{
				checkBoxPDF.CheckedChanged += eventHandler;
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
		//IL_0091: Unknown result type (might be due to invalid IL or missing references)
		//IL_009b: Expected O, but got Unknown
		//IL_010d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0117: Expected O, but got Unknown
		//IL_0187: Unknown result type (might be due to invalid IL or missing references)
		//IL_0191: Expected O, but got Unknown
		//IL_01b5: Unknown result type (might be due to invalid IL or missing references)
		//IL_0226: Unknown result type (might be due to invalid IL or missing references)
		//IL_0230: Expected O, but got Unknown
		//IL_02c3: Unknown result type (might be due to invalid IL or missing references)
		//IL_02cd: Expected O, but got Unknown
		//IL_035c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0366: Expected O, but got Unknown
		//IL_0387: Unknown result type (might be due to invalid IL or missing references)
		//IL_0410: Unknown result type (might be due to invalid IL or missing references)
		//IL_041a: Expected O, but got Unknown
		//IL_0495: Unknown result type (might be due to invalid IL or missing references)
		//IL_049f: Expected O, but got Unknown
		//IL_05d2: Unknown result type (might be due to invalid IL or missing references)
		//IL_05dc: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormViber));
		Label2 = new Label();
		telT = new TextBox();
		OkB = new Button();
		TextBox1 = new TextBox();
		OstT = new TextBox();
		NoB = new Button();
		Label1 = new Label();
		CheckBoxPDF = new CheckBox();
		((Control)this).SuspendLayout();
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(13, 152);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(182, 25);
		((Control)Label2).TabIndex = 10;
		Label2.Text = "Номер телефону:";
		((Control)telT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)telT).Location = new Point(135, 180);
		((Control)telT).Name = "telT";
		((Control)telT).Size = new Size(352, 30);
		((Control)telT).TabIndex = 8;
		telT.TextAlign = (HorizontalAlignment)2;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(277, 257);
		((Control)OkB).Margin = new Padding(3, 2, 3, 2);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(210, 39);
		((Control)OkB).TabIndex = 9;
		((ButtonBase)OkB).Text = "Надіслати ";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)TextBox1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox1).Location = new Point(18, 180);
		((Control)TextBox1).Name = "TextBox1";
		((TextBoxBase)TextBox1).ReadOnly = true;
		((Control)TextBox1).Size = new Size(102, 30);
		((Control)TextBox1).TabIndex = 11;
		((Control)TextBox1).TabStop = false;
		TextBox1.Text = "+38";
		TextBox1.TextAlign = (HorizontalAlignment)2;
		((Control)OstT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OstT).Location = new Point(18, 64);
		OstT.Multiline = true;
		((Control)OstT).Name = "OstT";
		((TextBoxBase)OstT).ReadOnly = true;
		((Control)OstT).Size = new Size(469, 71);
		((Control)OstT).TabIndex = 12;
		((Control)OstT).TabStop = false;
		OstT.TextAlign = (HorizontalAlignment)2;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(18, 257);
		((Control)NoB).Margin = new Padding(3, 2, 3, 2);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(210, 39);
		((Control)NoB).TabIndex = 13;
		((ButtonBase)NoB).Text = "Скасувати ";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)NoB).UseWaitCursor = true;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(13, 36);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(232, 25);
		((Control)Label1).TabIndex = 14;
		Label1.Text = "Повідомлення сервера:";
		((ButtonBase)CheckBoxPDF).AutoSize = true;
		((Control)CheckBoxPDF).Font = new Font("Microsoft Sans Serif", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CheckBoxPDF).Location = new Point(302, 12);
		((Control)CheckBoxPDF).Name = "CheckBoxPDF";
		((Control)CheckBoxPDF).Size = new Size(185, 22);
		((Control)CheckBoxPDF).TabIndex = 15;
		((ButtonBase)CheckBoxPDF).Text = "Використовувати PDF";
		((ButtonBase)CheckBoxPDF).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(505, 317);
		((Control)this).Controls.Add((Control)(object)CheckBoxPDF);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OstT);
		((Control)this).Controls.Add((Control)(object)TextBox1);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)telT);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormViber";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Надсилання чека ";
		((Form)this).TopMost = true;
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormViber(string nCh)
	{
		((Form)this).Load += FormViber_Load;
		CneckTaxN = "";
		InitializeComponent();
		CneckTaxN = nCh;
	}

	private void FormViber_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
		InViber inViber = new InViber();
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		typErr = inViber.InTextViber();
		if (typErr.errCode > 0)
		{
			OstT.Text = typErr.errStr;
			((Control)OkB).Enabled = false;
		}
		else if (Versioned.IsNumeric((object)typErr.errStr))
		{
			OstT.Text = "Залишок відправок: " + typErr.errStr;
			if (Conversions.ToInteger(typErr.errStr) > 0)
			{
				((Control)OkB).Enabled = true;
			}
			else
			{
				((Control)OkB).Enabled = false;
			}
		}
		else
		{
			OstT.Text = "Помилка!";
			((Control)OkB).Enabled = false;
		}
		if (Operators.CompareString(All.f.StringGetFn(All.A.FN, "PDF"), "1", false) == 0)
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
		((Form)this).Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		((Control)OkB).Enabled = false;
		string text = "38";
		if (telT.Text.Trim().Length != 10)
		{
			OstT.Text = "Вкажіть правильний номер телефону";
			((Control)telT).Focus();
			((Control)OkB).Enabled = true;
			return;
		}
		if (!Versioned.IsNumeric((object)telT.Text.Trim()))
		{
			OstT.Text = "Вкажіть правильний номер телефону";
			((Control)telT).Focus();
			((Control)OkB).Enabled = true;
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
		((Control)OkB).Enabled = true;
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
